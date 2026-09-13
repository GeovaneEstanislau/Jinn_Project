import os
import io
from pycdlib import PyCdlib

ROOT = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'iso_root')
ISO_PATH = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'jinn.iso')
LIMINE_BIN = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'limine', 'limine-bios-x86_64.bin')

p = PyCdlib()
p.new(interchange_level=3, joliet=True, rock_ridge='1.10', vol_ident='JINN')

def iso_name(name):
    s = ''
    for c in name.upper():
        if c.isalnum() or c == '_':
            s += c
        else:
            s += '_'
    return s[:30]

for dirpath, dirnames, filenames in os.walk(ROOT):
    rel = os.path.relpath(dirpath, ROOT)
    if rel == '.':
        iso_dir = '/'
        iso_dir_iso = '/'
    else:
        parts = rel.split(os.sep)
        iso_dir_iso = '/' + '/'.join(iso_name(p) for p in parts)
        try:
            p.add_directory(iso_dir_iso, rr_name=os.path.basename(rel))
        except Exception:
            pass
    for f in filenames:
        src = os.path.join(dirpath, f)
        iso_file_iso = iso_dir_iso.rstrip('/') + '/' + iso_name(f) + ';1'
        try:
            p.add_file(src, iso_file_iso, rr_name=f)
        except Exception:
            with open(src, 'rb') as fp:
                data = fp.read()
                p.add_fp(io.BytesIO(data), len(data), iso_file_iso, rr_name=f)

p.write(ISO_PATH)
p.close()
print('Wrote', ISO_PATH)
