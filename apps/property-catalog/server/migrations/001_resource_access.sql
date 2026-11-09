BEGIN;
CREATE TABLE IF NOT EXISTS app_migrations (version TEXT PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT now());
CREATE TABLE IF NOT EXISTS resource_owners (
 resource_type TEXT NOT NULL CHECK(resource_type IN ('property','landlord')),
 resource_id BIGINT NOT NULL, user_id BIGINT NOT NULL REFERENCES users(id), PRIMARY KEY(resource_type,resource_id,user_id));
CREATE TABLE IF NOT EXISTS contact_connections (
 id BIGSERIAL PRIMARY KEY, requester_id BIGINT NOT NULL REFERENCES users(id), recipient_id BIGINT NOT NULL REFERENCES users(id),
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','accepted')), created_at TIMESTAMPTZ NOT NULL DEFAULT now(), CHECK(requester_id<>recipient_id));
CREATE UNIQUE INDEX IF NOT EXISTS contact_pair ON contact_connections(LEAST(requester_id,recipient_id),GREATEST(requester_id,recipient_id));
CREATE TABLE IF NOT EXISTS resource_grants (
 resource_type TEXT NOT NULL CHECK(resource_type IN ('property','task')), resource_id BIGINT NOT NULL,
 user_id BIGINT NOT NULL REFERENCES users(id), granted_by BIGINT NOT NULL REFERENCES users(id), permissions TEXT[] NOT NULL,
 PRIMARY KEY(resource_type,resource_id,user_id));
ALTER TABLE users ADD COLUMN IF NOT EXISTS onboarding_completed BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE users ADD COLUMN IF NOT EXISTS interests TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE users ADD COLUMN IF NOT EXISTS installed_apps TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE leases ADD COLUMN IF NOT EXISTS tenant_user_id BIGINT REFERENCES users(id);
ALTER TABLE leases ADD COLUMN IF NOT EXISTS signer_user_id BIGINT REFERENCES users(id);
ALTER TABLE housework_tickets ADD COLUMN IF NOT EXISTS assignee_user_id BIGINT REFERENCES users(id);
CREATE TABLE IF NOT EXISTS legacy_task_assignments(task_id BIGINT PRIMARY KEY, assignee TEXT NOT NULL);
DO $$
DECLARE fallback BIGINT;
BEGIN
 IF NOT EXISTS(SELECT 1 FROM app_migrations WHERE version='001_resource_access') THEN
  IF EXISTS(SELECT 1 FROM properties p WHERE NOT EXISTS(SELECT 1 FROM users u WHERE u.landlord_id=p.landlord_id AND u.role='landlord' AND u.active)) THEN
   IF (SELECT count(*) FROM users WHERE role='admin' AND active)<>1 THEN RAISE EXCEPTION 'Resource migration needs one active administrator for unlinked properties'; END IF;
   SELECT id INTO fallback FROM users WHERE role='admin' AND active;
  END IF;
  INSERT INTO resource_owners SELECT 'property',p.id,u.id FROM properties p JOIN users u ON u.landlord_id=p.landlord_id AND u.role='landlord' AND u.active ON CONFLICT DO NOTHING;
  INSERT INTO resource_owners SELECT 'property',p.id,fallback FROM properties p WHERE NOT EXISTS(SELECT 1 FROM resource_owners o WHERE o.resource_type='property' AND o.resource_id=p.id) ON CONFLICT DO NOTHING;
  INSERT INTO resource_owners SELECT DISTINCT 'landlord',p.landlord_id,o.user_id FROM properties p JOIN resource_owners o ON o.resource_type='property' AND o.resource_id=p.id ON CONFLICT DO NOTHING;
  INSERT INTO legacy_task_assignments SELECT id,assignee FROM housework_tickets WHERE assignee<>'' ON CONFLICT DO NOTHING;
  UPDATE housework_tickets SET assignee='',assignee_user_id=NULL WHERE assignee<>'';
  UPDATE leases l SET tenant_user_id=u.id FROM users u WHERE lower(u.email)=lower(l.tenant_email);
  UPDATE leases l SET signer_user_id=u.id FROM users u WHERE lower(u.email)=lower(l.landlord_snapshot #>> '{landlord,email}');
  ALTER TABLE users DROP CONSTRAINT IF EXISTS users_role_check;
  UPDATE users SET role='user',auth_version=auth_version+1 WHERE role<>'admin';
  INSERT INTO app_migrations(version) VALUES('001_resource_access');
 END IF;
END $$;
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_role_check;
ALTER TABLE users ADD CONSTRAINT users_role_check CHECK(role IN ('user','admin'));
CREATE OR REPLACE FUNCTION buildry_property_owner(actor BIGINT, target BIGINT) RETURNS BOOLEAN LANGUAGE SQL STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM resource_owners WHERE resource_type='property' AND resource_id=target AND user_id=actor)
$$;
CREATE OR REPLACE FUNCTION buildry_property_can(actor BIGINT,target BIGINT,cap TEXT) RETURNS BOOLEAN LANGUAGE SQL STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM users u WHERE u.id=actor AND u.active AND (u.role='admin' OR buildry_property_owner(actor,target) OR EXISTS(
 SELECT 1 FROM resource_grants g WHERE g.resource_type='property' AND g.resource_id=target AND g.user_id=actor AND (cap=ANY(g.permissions) OR (cap='leases.read' AND 'leases.approve'=ANY(g.permissions)) OR (right(cap,5)='.read' AND replace(cap,'.read','.write')=ANY(g.permissions))))))
$$;
CREATE OR REPLACE FUNCTION buildry_task_can(actor BIGINT,target BIGINT,writing BOOLEAN) RETURNS BOOLEAN LANGUAGE SQL STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM users u JOIN housework_tickets h ON h.id=target WHERE u.id=actor AND u.active AND
 (u.role='admin' OR h.creator_id=actor OR h.assignee_user_id=actor OR
 (h.property_id IS NOT NULL AND buildry_property_can(actor,h.property_id,CASE WHEN writing THEN 'tasks.write' ELSE 'tasks.read' END)) OR
 EXISTS(SELECT 1 FROM resource_grants g WHERE g.resource_type='task' AND g.resource_id=h.id AND g.user_id=actor AND (CASE WHEN writing THEN 'work' ELSE 'view' END=ANY(g.permissions) OR 'work'=ANY(g.permissions))) OR
 (NOT writing AND EXISTS(SELECT 1 FROM tenant_maintenance_requests r WHERE r.task_id=h.id AND r.tenant_id=actor))))
$$;
COMMIT;
