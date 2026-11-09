#!/usr/bin/env python3
"""API regression checks against seed_api_review_accounts in a disposable *_test database.

Start the native API on port 8089 with TEST_DATABASE_URL's database, then run this.
No external email or payment provider is called.
"""
import http.cookiejar
import json
import base64
import subprocess
import urllib.error
import urllib.parse
import urllib.request

BASE = 'http://127.0.0.1:8089'
PASSWORD = 'Resource-review-only-2026!'

class Client:
    def __init__(self, label):
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
        body = urllib.parse.urlencode({'email': f'review-{label}@example.test', 'password': PASSWORD}).encode()
        self.opener.open(urllib.request.Request(BASE+'/login', data=body), timeout=20).read()
        self.me = self.call('GET', '/api/me')[1]
        assert self.me['role'] == 'user'

    def call(self, method, path, body=None, status=200):
        data = None if body is None else json.dumps(body).encode()
        req = urllib.request.Request(BASE+path, data=data, method=method,
                                     headers={'Content-Type':'application/json'})
        try:
            with self.opener.open(req, timeout=20) as response:
                actual, raw = response.status, response.read().decode()
        except urllib.error.HTTPError as error:
            actual, raw = error.code, error.read().decode()
        assert actual == status, (method, path, actual, status, raw[:500])
        try: result = json.loads(raw)
        except ValueError: result = raw
        return actual, result

owner, worker, stranger = (Client(label) for label in ('owner', 'worker', 'stranger'))
owner.call('PUT','/api/account/preferences',{'interests':['tenant','owner'], 'installed_apps':['properties','rentals','tasks','leases','payments']},204)
worker.call('PUT','/api/account/preferences',{'interests':['worker'], 'installed_apps':['properties','leases','payments','tasks']},204)
worker.call('PUT','/api/account/preferences',{'interests':['admin'], 'installed_apps':['tasks']},400)
assert stranger.call('GET','/api/properties')[1] == []
assert stranger.call('GET','/api/leases')[1] == []
stranger.call('GET','/api/access/users',status=403)
# Remove connections from previous runs, including their dependent permissions.
for contact in owner.call('GET','/api/contacts')[1]:
    if contact['user_id'] == worker.me['id']:
        owner.call('DELETE',f"/api/contacts/{contact['id']}",{},204)

task = owner.call('POST','/api/tasks',dict(title='Resource access regression', description='', property_id=None,
                 assignee='', status='backlog', start_date=None,start_time=None,estimate_minutes=None,photos=[]),200)[1]
task_id = task['id']
grant = dict(user_id=worker.me['id'], permissions=['work'])
owner.call('PUT',f'/api/sharing/task/{task_id}',grant,403)
owner.call('POST','/api/contacts',{'email':worker.me['email']},201)
owner.call('POST','/api/contacts',{'email':worker.me['email']},409)
contact = next(c for c in worker.call('GET','/api/contacts')[1] if c['user_id']==owner.me['id'])
owner.call('POST',f"/api/contacts/{contact['id']}/accept",{},404)
worker.call('POST',f"/api/contacts/{contact['id']}/accept",{},204)
# View access allows reads but no mutation.
owner.call('PUT',f'/api/sharing/task/{task_id}',dict(user_id=worker.me['id'],permissions=['view']),204)
visible = next(t for t in worker.call('GET','/api/tasks')[1] if t['id']==task_id)
assert not visible['can_edit'] and not visible['can_share']
edit = dict(title='Updated by worker',description='',property_id=None,assignee='',status='backlog',
            start_date=None,start_time=None,estimate_minutes=None,photos=[])
worker.call('PUT',f'/api/tasks/{task_id}',edit,404)
owner.call('PUT',f'/api/sharing/task/{task_id}',grant,204)
worker.call('PUT',f'/api/tasks/{task_id}',edit)
worker.call('PUT',f'/api/tasks/{task_id}',dict(edit,property_id=1),404)
worker.call('PUT',f'/api/sharing/task/{task_id}',dict(user_id=stranger.me['id'],permissions=['work']),404)
assert worker.call('GET','/api/properties')[1] == []
# Function grants never turn into broader property access.
owner.call('PUT','/api/sharing/property/1',dict(user_id=worker.me['id'],permissions=['pricing.write']),204)
assert len(worker.call('GET','/api/properties?function=pricing')[1]) == 1
assert worker.call('GET','/api/properties')[1] == []
assert worker.call('GET','/api/properties?function=leases')[1] == []
worker.call('GET','/api/properties/1/access',status=404)
owner.call('PUT','/api/sharing/property/1',dict(user_id=worker.me['id'],permissions=['leases.write']),204)
assert len(worker.call('GET','/api/properties?function=leases')[1]) == 1
worker.call('POST','/api/leases/999999/confirm',{},404)
worker.call('GET','/api/my-rentals/999999/document',status=404)
worker.call('PUT','/api/landlords/1',dict(id=1,legal_name='Unauthorized',service_address='',email='',phone='',signer_name='',signer_title='',signature_authorized=False,property_ids=[]),404)
# Profile updates cannot change role or resource permissions.
worker.call('PUT','/api/account/profile',dict(first_name='Worker',last_name='Review',phone='519-555-0142'),204)
assert worker.call('GET','/api/me')[1]['phone'] == '519-555-0142'
# Profile images follow the accepted connection rather than becoming public uploads.
png=base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9ZlSAAAAAASUVORK5CYII=')
with worker.opener.open(urllib.request.Request(BASE+'/api/tasks/images',data=png,headers={'Content-Type':'image/png'})) as response:
    photo=json.loads(response.read())
worker.call('PUT','/api/account/profile',dict(first_name='Worker',last_name='Review',phone='519-555-0142',photo_url=photo),204)
with owner.opener.open(BASE+photo) as response:
    assert response.status==200 and 'no-store' in response.headers['Cache-Control']
stranger.call('GET',photo,status=404)
owner.call('DELETE',f"/api/contacts/{contact['id']}",{},204)
owner.call('GET',photo,status=404)
assert not any(t['id']==task_id for t in worker.call('GET','/api/tasks')[1])
assert worker.call('GET','/api/properties?function=leases')[1] == []
owner.call('DELETE',f'/api/tasks/{task_id}',{},204)
# Creating from the same client-side candidate ID allocates independent server IDs.
original=owner.call('GET','/api/properties')[1][0]
candidate=dict(original,name='Access regression property',address='100 Example Street, Waterloo, ON',spaces=[],photos=[],notes='')
first=stranger.call('POST','/api/properties',candidate)[1]
second=stranger.call('POST','/api/properties',candidate)[1]
assert first['id']!=second['id'] and first['id']!=original['id']
assert 'details.write' in first['capabilities']
assert not any(p['id']==first['id'] for p in owner.call('GET','/api/properties')[1])
# Clean only these synthetic properties in the explicitly named disposable database.
ids=','.join(str(p['id']) for p in [first,second])
cleanup=f"DELETE FROM resource_owners WHERE (resource_type='property' AND resource_id IN ({ids})) OR (resource_type='landlord' AND resource_id IN (SELECT landlord_id FROM properties WHERE id IN ({ids}))); DELETE FROM properties WHERE id IN ({ids}) AND data->>'name'='Access regression property';"
subprocess.run(['docker','exec','buildry-property-postgres','psql','-U','buildry','-d','buildry_unified_test','-X','-v','ON_ERROR_STOP=1','-q','-c',cleanup],check=True,capture_output=True)
# Session invalidation applies immediately.
worker.call('POST','/api/account/sessions/revoke',{},204)
worker.call('GET','/api/me',status=401)
print('PASS: onboarding, apps, contacts, standalone tasks, view/work sharing, function grants, revocation, account settings, and session invalidation')
