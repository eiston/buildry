#!/usr/bin/env python3
"""Isolated task UI fixtures on port 8082; API writes remain in memory."""
import importlib.util
from pathlib import Path
from urllib.parse import urlparse, parse_qs
from datetime import datetime
from http.server import ThreadingHTTPServer

spec = importlib.util.spec_from_file_location('tour_preview', Path(__file__).with_name('preview-tours.py'))
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
PROPERTIES = [dict(id=h['id'], name=h['name'], address=h['address'], notes='', photos=['/assets/stay-house-modern.jpg', '/assets/stay-house-red-brick.jpg'] if h['id']==1 else [], owner_name='Demo Owner', owner_manages_tours=False, manager_staff_ids=[101,102]) for h in base.HOUSES]
USERS = [dict(id=101,first_name='Alex',last_name='Morgan',role='user',active=True),dict(id=102,first_name='Jamie',last_name='Chen',role='user',active=True)]
TASKS = [dict(id=i,creator_id=101,title=title,description=description,property_id=house,property_name=PROPERTIES[house-1]['name'],assignee='Alex Morgan' if i%2 else 'Jamie Chen',status=status,start_date=str(base.TODAY) if status!='backlog' else None,start_time='10:00' if status!='backlog' else None,estimate_minutes=60 if status!='backlog' else None,photos=['/assets/stay-house-modern.jpg'] if i==3 else [],created_at=str(base.TODAY),updated_at=str(base.TODAY),tenant_request=i==2) for i,title,status,house,description in [(1,'Replace the kitchen faucet','backlog',1,'Check the shut-off valve and bring a replacement cartridge.'),(2,'Repair the bedroom window latch','planned',2,'The latch no longer catches. Tenant is available after 10 AM.'),(3,'Prepare the garden room for move-in','in_progress',1,'Clean the room and inspect fixtures.'),(4,'Inspect smoke detectors','done',2,'All detectors tested.'),(5,'Replace hallway light fittings','aborted',3,'Awaiting a revised work order.')]]

class TaskPreview(base.Preview):
    def do_GET(self):
        path=urlparse(self.path).path
        scenario=parse_qs(urlparse(self.headers.get('Referer','')).query).get('preview',[''])[0]
        if path.endswith('/access') and path.startswith('/api/properties/'):
            return self.respond(dict(can_edit=True,can_manage=True))
        if path=='/api/property-managers/staff': return self.respond([dict(id=u['id'],name=u['first_name']+' '+u['last_name'],email='manager@example.test',active=True) for u in USERS])
        if path.startswith('/assets/stay-house-'):
            asset=Path(__file__).resolve().parent.parent / path.lstrip('/')
            if asset.is_file():
                data=asset.read_bytes(); self.send_response(200); self.send_header('Content-Type','image/jpeg'); self.send_header('Content-Length',str(len(data))); self.end_headers(); self.wfile.write(data); return
        if path=='/api/me': return self.respond(dict(id=101,role='admin',onboarding_completed=True,interests=['owner'],installed_apps=['properties','tasks','leases','payments']))
        if path=='/api/tasks':
            if scenario=='error': return self.respond('Could not load tasks.',503)
            return self.respond([] if scenario=='empty' else TASKS)
        if path=='/api/tasks/properties' or path=='/api/properties': return self.respond(PROPERTIES)
        if path=='/api/tasks/users': return self.respond(USERS)
        return super().do_GET()
    def mutate(self):
        path=urlparse(self.path).path
        if path=='/api/tasks/images':
            self.rfile.read(int(self.headers.get('Content-Length',0)))
            return self.respond('/assets/stay-house-modern.jpg')
        if path.startswith('/api/tasks'):
            data=base.json.loads(self.rfile.read(int(self.headers.get('Content-Length',0))) or '{}')
            if path=='/api/tasks' and self.command=='POST':
                task=dict(id=max([t['id'] for t in TASKS]+[0])+1,creator_id=101,created_at=str(base.TODAY),updated_at=str(base.TODAY),tenant_request=False)
                TASKS.append(task)
            else:
                task=next((t for t in TASKS if t['id']==int(path.rsplit('/',1)[-1])),None)
                if task is None: return self.respond('Task not found.',404)
            if self.command=='DELETE':
                TASKS.remove(task)
                return self.respond({})
            task.update(data)
            task['property_name']=next((p['name'] for p in PROPERTIES if p['id']==data.get('property_id')),'')
            return self.respond(task)
        return self.respond('Unsupported mock write.',404)
    do_POST=mutate
    do_PUT=mutate
    do_DELETE=mutate

if __name__=='__main__':
    print('Mock tasks: http://127.0.0.1:8082/tasks',flush=True)
    ThreadingHTTPServer(('127.0.0.1',8082),TaskPreview).serve_forever()
