#!/usr/bin/env python3
"""Lease payment regression against review accounts and a confirmed lease fixture.
Run a native API without Stripe credentials against a disposable *_test database.
PAYMENT_TEST_DATABASE names that database; PAYMENT_TEST_URL defaults to port 8098.
"""
import http.cookiejar
import json
import os
import subprocess
import urllib.error
import urllib.parse
import urllib.request

DATABASE = os.environ['PAYMENT_TEST_DATABASE']
assert DATABASE.endswith('_test'), 'Use a disposable test database'
BASE = os.environ.get('PAYMENT_TEST_URL', 'http://127.0.0.1:8098')

def sql(query):
    return subprocess.check_output([
        'docker', 'exec', 'buildry-property-postgres', 'psql', '-U', 'buildry',
        '-d', DATABASE, '-X', '-v', 'ON_ERROR_STOP=1', '-At', '-c', query,
    ], text=True).strip()

class Client:
    def __init__(self, label):
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
        body = urllib.parse.urlencode({'email': f'review-{label}@example.test', 'password': 'Resource-review-only-2026!'}).encode()
        self.opener.open(urllib.request.Request(BASE + '/login', data=body), timeout=20).read()

    def get(self, path, status=200):
        try:
            with self.opener.open(BASE + path, timeout=20) as response:
                actual, raw = response.status, response.read()
        except urllib.error.HTTPError as error:
            actual, raw = error.code, error.read()
        assert actual == status, (path, actual, status, raw[:300])
        return json.loads(raw) if status == 200 else None

ids = []
try:
    for amount in ['111', '222']:
        value = sql(f"""INSERT INTO leases(tenant_name,tenant_email,tenant_user_id,property_id,room_id,details,landlord_snapshot,status)
            SELECT tenant_name,tenant_email,tenant_user_id,property_id,room_id,
            details || '{{"rent_deposit":"{amount}"}}'::jsonb,landlord_snapshot,'confirmed'
            FROM leases WHERE tenant_email='review-tenant@example.test' AND status='confirmed' ORDER BY id LIMIT 1 RETURNING id""")
        lease = int(value.splitlines()[0])
        ids.append(lease)
        sql(f"""INSERT INTO lease_deposit_receipts(lease_id,kind,received_on,recorded_by)
            SELECT {lease},'rent','2026-11-13',id FROM users WHERE email='review-owner@example.test'""")
    owner, tenant, stranger = [Client(label) for label in ['owner', 'tenant', 'stranger']]
    for client, direction in [(owner, 'in'), (tenant, 'out')]:
        generic = client.get('/api/payments')
        assert all(any(row['id'].startswith(f'lease-{lease}-') for row in generic['rows']) for lease in ids)
        for lease, amount in zip(ids, [11100, 22200]):
            account = client.get(f'/api/payments?lease={lease}')
            assert [item['id'] for item in account['leases']] == [lease]
            assert account['rows'] and all(row['id'].startswith(f'lease-{lease}-') for row in account['rows'])
            assert all(row['direction'] == direction for row in account['rows'])
            assert any(row['amount_cents'] == amount for row in account['rows'])
            assert all(party['lease_id'] == lease for party in account['parties'])
            stranger.get(f'/api/payments?lease={lease}', 404)
        client.get('/api/payments?lease=9223372036854775807', 404)
        client.get('/api/payments?lease=invalid', 400)
    sql(f"UPDATE leases SET status='signed' WHERE id={ids[1]}")
    assert owner.get(f'/api/payments?lease={ids[1]}')['rows']
    tenant.get(f'/api/payments?lease={ids[1]}', 404)
    print('PASS: signed-owner history, two leases, owner/tenant history, unscoped activity, and unauthorized/malformed scope')
finally:
    for lease in ids:
        sql(f'DELETE FROM lease_deposit_receipts WHERE lease_id={lease}; DELETE FROM leases WHERE id={lease}')
