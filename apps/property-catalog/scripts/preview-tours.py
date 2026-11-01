#!/usr/bin/env python3
"""Isolated calendar/tour fixtures. Run after Compose on http://127.0.0.1:8081.
All API writes stay in memory; restarting resets the fixtures. No emails or DB writes.
"""
import copy
import json
from datetime import datetime, timedelta
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs
from urllib.request import urlopen
from zoneinfo import ZoneInfo

TODAY = datetime.now(ZoneInfo('America/Toronto')).date()
HOUSES = [dict(id=i, name=name, address=address, available=i != 3,
               rooms=[dict(id=i * 10 + j, name=room, monthly_rent=950 + j * 150) for j, room in enumerate(['Garden room', 'Ensuite room', 'Private suite'], 1)])
          for i, name, address in [(1, 'Garden House', '120 Garden Street'), (2, 'Cedar House', '42 Cedar Avenue'), (3, 'Willow House', '86 Willow Lane')]]

def make_booking(i, offset, slot, name, house=1):
    return dict(id=i, property_id=house, property_name=HOUSES[house-1]['name'],
                tour_date=str(TODAY + timedelta(days=offset)), slot_start=slot,
                guest_name=name, guest_phone='519-555-0100', guest_email=f'guest{i}@example.test',
                room_id=house*10+1, room_name='Garden room', rental_start=f'{TODAY.year+1}-01-01', rental_year=TODAY.year+1,
                rental_months=8, manager_name='Demo Tour Host', manager_notice_sent=True,
                quoted_monthly_rent=1100, quote_is_estimate=True, is_demo=True)
BOOKINGS = [make_booking(1, 0, '16:00', 'Alex Morgan'), make_booking(2, 1, '10:00', 'Jamie Chen', 2),
            make_booking(3, 2, '14:00', 'Taylor Patel'), make_booking(4, -2, '12:00', 'Sam Rivera', 2),
            make_booking(5, 4, '11:00', 'Avery Williams'), make_booking(6, 0, '10:00', 'Jordan Lee', 2)]
CANCELLED = set()
TOKENS = {'demo-tour': 1, 'demo-cancelled': 4}
CANCELLED.add(4)

class Preview(BaseHTTPRequestHandler):
    def respond(self, value, status=200):
        data = json.dumps(value).encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        path = urlparse(self.path).path
        query = parse_qs(urlparse(self.path).query)
        if path == '/api/tours/options':
            return self.respond(dict(houses=[h for h in HOUSES if h['available']], today=str(TODAY), last_date=str(TODAY+timedelta(days=60)), now_time=datetime.now(ZoneInfo('America/Toronto')).strftime('%H:%M')))
        if path == '/api/tours/houses': return self.respond(HOUSES)
        if path == '/api/tours': return self.respond([b for b in BOOKINGS if b['id'] not in CANCELLED])
        if path == '/api/tours/availability':
            day = query.get('date', [''])[0]
            excluded = int(query.get('exclude_booking_id', ['0'])[0])
            occupied = [int(b['slot_start'][:2]) for b in BOOKINGS if b['tour_date']==day and b['id'] != excluded and b['id'] not in CANCELLED]
            slots = [f'{h:02d}:00' for h in range(10,19) if all(abs(h-o)>=2 for o in occupied)]
            if day == str(TODAY+timedelta(days=3)): slots=[]
            return self.respond(dict(available_slots=slots))
        if path.startswith('/api/guest-bookings/'):
            booking = next((b for b in BOOKINGS if b['id']==TOKENS.get(path.rsplit('/',1)[-1])), None)
            return self.respond(dict(booking=booking, cancelled=booking['id'] in CANCELLED)) if booking else self.respond('Booking not found.',404)
        if path.startswith('/api/'):
            return self.respond({'revision':'tour-preview'} if path=='/api/ui-revision' else [])
        # Reuse the real built application. Never forward API calls or writes.
        upstream = self.path if path.startswith(('/assets/', '/icons/')) or '.' in path.rsplit('/',1)[-1] else '/stays'
        try:
            with urlopen('http://127.0.0.1:8080'+upstream) as response:
                data=response.read(); self.send_response(response.status)
                self.send_header('Content-Type',response.headers.get('Content-Type','application/octet-stream'))
                self.send_header('Content-Length',str(len(data))); self.end_headers(); self.wfile.write(data)
        except Exception:
            self.respond('Start the local Compose app first.',502)

    def mutate(self):
        path=urlparse(self.path).path
        data=json.loads(self.rfile.read(int(self.headers.get('Content-Length',0))) or '{}')
        if path.startswith('/api/tours/houses/'):
            house=next(h for h in HOUSES if h['id']==int(path.rsplit('/',1)[-1])); house['available']=data['available']; return self.respond(house)
        if self.command=='POST' and path=='/api/tours':
            i=max(b['id'] for b in BOOKINGS)+1
            booking=make_booking(i,0,data['slot_start'],data['legal_first_name']+' '+data['legal_last_name'],data['property_id'])
            booking.update({k:v for k,v in data.items() if k not in ['legal_first_name','legal_last_name']})
            BOOKINGS.append(booking); token=f'demo-tour-{i}'; TOKENS[token]=i
            return self.respond(dict(booking=booking,token=token))
        guest=path.startswith('/api/guest-bookings/')
        i=TOKENS.get(path.rsplit('/',1)[-1]) if guest else int(path.rsplit('/',1)[-1])
        booking=next(b for b in BOOKINGS if b['id']==i)
        if self.command=='DELETE': CANCELLED.add(i); return self.respond({})
        booking.update(data)
        return self.respond(dict(booking=booking,cancelled=i in CANCELLED) if guest else booking)
    do_POST=mutate
    do_PUT=mutate
    do_DELETE=mutate

if __name__=='__main__':
    print('Mock tour preview: http://127.0.0.1:8081/calendar',flush=True)
    ThreadingHTTPServer(('127.0.0.1',8081),Preview).serve_forever()
