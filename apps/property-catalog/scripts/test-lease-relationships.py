#!/usr/bin/env python3
"""Full lease-party regression using only local synthetic data in buildry_unified_test.
Start the review API with PHOTO_ID_ENCRYPTION_KEY set to a disposable 32-byte base64 key.
No email or Stripe credentials are needed; the send transition is simulated locally.
"""
import hashlib
import urllib.error
import re
import json
import runpy
import struct
import subprocess
import urllib.parse
import urllib.request
import uuid
import zlib

shared = runpy.run_path(str(__file__).replace('test-lease-relationships.py','test-resource-access.py'))
Client = shared['Client']
BASE = shared['BASE']
owner, tenant, stranger, worker = [Client(v) for v in ('owner','tenant','stranger','worker')]

def sql(query):
    return subprocess.check_output(['docker','exec','buildry-property-postgres','psql','-U','buildry','-d','buildry_unified_test','-X','-v','ON_ERROR_STOP=1','-At','-c',query],text=True).strip()

sql("DELETE FROM lease_photo_id_access WHERE lease_id IN (SELECT id FROM leases WHERE tenant_email='review-tenant@example.test'); DELETE FROM leases WHERE tenant_email='review-tenant@example.test';")
profile = dict(id=0,legal_name="Review Legal Landlord",service_address="",email="",phone="",signer_name="",signer_title="",signature_authorized=False,property_ids=[1])
profile.update(email=owner.me['email'], signer_name='owner Review', signer_title='Owner',signature_authorized=True,
               service_address='100 Example Street, Waterloo, ON N2L 0A0')
profile=owner.call('POST','/api/landlords',profile)[1]
terms=dict(start_date='2030-01-01',end_date='2030-12-31',term_type='fixed',monthly_rent='950',rent_due_day='1',
           rent_payment_method='E-transfer',parking='None',condo='No',electricity_payer='landlord',heat_payer='landlord',water_payer='landlord')
lease=owner.call('POST','/api/leases',dict(tenant_name='tenant Review',tenant_email=tenant.me['email'],property_id=1,room_id=0,lease_type='entire_house',details=terms))[1]
id=lease['id']
secret=uuid.uuid4().hex+uuid.uuid4().hex
digest=hashlib.sha256(secret.encode()).hexdigest()
# Only simulate delivery, leaving real authentication/signing/ID/confirmation handlers under test.
sql(f"UPDATE leases SET status='sent',token_hash='{digest}',token_expires_at=now()+interval '14 days' WHERE id={id};")
assert any(r['id']==id for r in tenant.call('GET','/api/my-rentals')[1])
stranger.call('GET',f'/lease/{secret}',status=403)
stranger.call('GET',f'/lease/{secret}/document',status=403)
stranger.call('POST',f'/lease/{secret}',dict(signature='tenant Review',consent='yes'),403)
# Anonymous invitations require sign-in rather than returning the lease body.
with urllib.request.urlopen(BASE+f'/lease/{secret}') as response:
    assert '/login?' in response.url

def form(client,path,values):
    body=urllib.parse.urlencode(values).encode()
    req=urllib.request.Request(BASE+path,data=body,headers={'Content-Type':'application/x-www-form-urlencoded'})
    with client.opener.open(req,timeout=30) as response:
        return response.status,response.url,response.read()

form(tenant,f'/lease/{secret}',dict(signature='tenant Review',consent='yes'))
assert next(l for l in owner.call('GET','/api/leases')[1] if l['id']==id)['status']=='signed'
owner.call('POST',f'/api/leases/{id}/confirm',{},409)
with owner.opener.open(BASE+f'/leases/{id}/sign') as response:
    signing_url=response.url
signer_secret=signing_url.rsplit('/',1)[1]
tenant.call('GET',f'/lease-landlord/{signer_secret}',status=403)
form(owner,f'/lease-landlord/{signer_secret}',dict(signature='owner Review',consent='yes'))
# Synthetic 600x600 patterned PNG: no real identity data.
def chunk(kind,data):
    return struct.pack('!I',len(data))+kind+data+struct.pack('!I',zlib.crc32(kind+data)&0xffffffff)
png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('!IIBBBBB',600,600,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b''.join(b'\0'+bytes(channel for x in range(600) for channel in (x%256,y%256,(x*y)%256)) for y in range(600))))+chunk(b'IEND',b'')
boundary='BuildryReview'+uuid.uuid4().hex
body=b''
for key,value in [('document_type','passport'),('terms','yes')]:
    body+=f'--{boundary}\r\nContent-Disposition: form-data; name="{key}"\r\n\r\n{value}\r\n'.encode()
body+=f'--{boundary}\r\nContent-Disposition: form-data; name="front"; filename="synthetic.png"\r\nContent-Type: image/png\r\n\r\n'.encode()+png+f'\r\n--{boundary}--\r\n'.encode()
try:
    with tenant.opener.open(urllib.request.Request(BASE+f'/lease/{secret}/photo-id',data=body,headers={'Content-Type':f'multipart/form-data; boundary={boundary}'}),timeout=30) as response:
        assert response.status==200
except urllib.error.HTTPError as error:
    text=error.read().decode()
    raise AssertionError((error.code,re.findall(r"photo-id-error[^>]*>(.*?)</p>",text),text[-700:]))
# A lease editor can prepare leases but cannot inspect ID or confirm a tenancy.
owner.call('POST','/api/contacts',dict(email=worker.me['email']),201)
contact=next(c for c in worker.call('GET','/api/contacts')[1] if c['user_id']==owner.me['id'])
worker.call('POST',f"/api/contacts/{contact['id']}/accept",{},204)
owner.call('PUT','/api/sharing/property/1',dict(user_id=worker.me['id'],permissions=['leases.write']),204)
worker.call('GET',f'/api/leases/{id}/photo-id/front',status=404)
worker.call('POST',f'/api/leases/{id}/photo-id/approve',{},404)
worker.call('POST',f'/api/leases/{id}/confirm',{},404)
owner.call('POST',f'/api/leases/{id}/photo-id/approve',{},409)
# Viewing the synthetic image creates the required audit entry.
with owner.opener.open(BASE+f'/api/leases/{id}/photo-id/front') as response:
    assert response.headers['Content-Type']=='image/png'
owner.call('POST',f'/api/leases/{id}/photo-id/approve',{},204)
owner.call('POST',f'/api/leases/{id}/confirm',{},204)
assert next(r for r in tenant.call('GET','/api/my-rentals')[1] if r['id']==id)['status']=='confirmed'
assert tenant.call('GET','/api/me')[1]['role']=='user'
with tenant.opener.open(BASE+f'/api/my-rentals/{id}/document') as response:
    assert response.read().startswith(b'%PDF')
stranger.call('GET',f'/api/my-rentals/{id}/document',status=404)
assert sql(f"SELECT tenant_user_id=(SELECT id FROM users WHERE email='review-tenant@example.test') AND signer_user_id=(SELECT id FROM users WHERE email='review-owner@example.test') FROM leases WHERE id={id}")=='t'
print('PASS: authenticated invitations, designated signing, tenant ID upload, separate approval rights, confirmation, signed PDF access, and stable user links')
