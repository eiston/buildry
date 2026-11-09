#!/usr/bin/env python3
"""Opt-in UI fixtures for localhost. Keeps existing rooms and uses reserved emails."""
import getpass
import http.cookiejar
import json
import os
import urllib.parse
import urllib.request

BASE = 'http://127.0.0.1:8080'
cookies = http.cookiejar.CookieJar()
client = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cookies))
password = os.environ.get('BUILDRY_ADMIN_PASSWORD') or getpass.getpass('Local admin password: ')
client.open(BASE + '/login', urllib.parse.urlencode({'email': 'admin@buildry.local', 'password': password}).encode()).read()


def request(path, body=None, method=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(BASE + path, data, {'Content-Type': 'application/json'}, method=method)
    with client.open(req) as response:
        return json.load(response)


users = request('/api/access/users')
manager = next((person for person in users if person['email'] == 'ui-review@example.test'), None)
if manager is None:
    manager = request('/api/access/users', {'first_name': 'UI Demo', 'last_name': 'Manager', 'email': 'ui-review@example.test', 'phone': '519-555-0100', 'role': 'user', 'active': True, 'landlord_id': None})
properties = request('/api/properties')
for house in properties[:2]:
    if house.get('spaces'):
        continue
    house['spaces'] = [dict(id=index, name=name, external_name=name.replace('Demo · ', ''), kind=kind, notes='Local UI demo', amenities=['Wi-Fi', 'Desk'], photos=[], bathroom_access='Ensuite' if index == 2 else 'Separate', current_monthly_rent=price, rent_is_estimate=True) for index, name, kind, price in [(9001, 'Demo · Garden room', 'Bedroom', 950), (9002, 'Demo · Ensuite room', 'Bedroom', 1150), (9003, 'Demo · Private suite', 'Suite', 1550)]]
    house['manager_staff_ids'] = sorted(set(house.get('manager_staff_ids', []) + [manager['id']]))
    house['owner_manages_tours'] = False
    request(f"/api/properties/{house['id']}", house, 'PUT')
    print(f"Added demo rooms to house {house['id']}")
