#!/usr/bin/env python3
"""Download an observed public MISUMI China static 2D ZIP; never generates 3D CAD."""
import argparse
import hashlib
import io
import json
from datetime import datetime, timezone
from pathlib import Path
import urllib.parse
import urllib.request
import zipfile
from nx_direct import safe_members

LIMIT = 10 * 1024 * 1024

def check_2d(name, data):
    suffix=Path(name).suffix.lower()
    if data.lstrip().lower().startswith((b'<!doctype html',b'<html')): raise ValueError('HTML is not CAD data')
    if suffix=='.dxf' and not (data.startswith(b'AutoCAD Binary DXF') or all(token in data for token in (b'SECTION',b'HEADER',b'EOF'))): raise ValueError('DXF content signature missing')
    if suffix=='.dwg' and not data.startswith(b'AC10'): raise ValueError('DWG content signature missing')
    if suffix=='.pdf' and not data.startswith(b'%PDF-'): raise ValueError('PDF content signature missing')

def validate_url(url):
    p=urllib.parse.urlsplit(url)
    path=urllib.parse.unquote(p.path)
    if p.scheme != 'https' or p.netloc != 'www.misumi.com.cn' or p.query or p.fragment or not path.startswith('/linked/material/') or not path.lower().endswith('.zip') or '..' in Path(path).parts:
        raise ValueError('Requires an observed public https://www.misumi.com.cn/linked/material/...zip URL without query or credentials')
    return url

class CheckedRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        validate_url(newurl)
        return super().redirect_request(req,fp,code,msg,headers,newurl)

def download(url, observed_from, out, expected_sha256=None):
    validate_url(url)
    if not observed_from.strip(): raise ValueError('Record where the exact URL was observed')
    out=Path(out)
    if out.exists(): raise ValueError('Output must be a new directory')
    opener=urllib.request.build_opener(CheckedRedirect())
    with opener.open(url,timeout=30) as response:
        validate_url(response.url)
        if response.status != 200: raise ValueError('HTTP status not 200')
        content=response.read(LIMIT+1)
    if len(content)>LIMIT: raise ValueError('ZIP exceeds 10 MiB intake limit')
    actual=hashlib.sha256(content).hexdigest()
    if expected_sha256 and actual != expected_sha256: raise ValueError('Expected file digest differs')
    with zipfile.ZipFile(io.BytesIO(content)) as z:
        safe_members(z)
        members=[i for i in z.infolist() if not i.is_dir()]
        if not members or not all(Path(i.filename).suffix.lower() in ('.dxf','.dwg','.pdf','.txt') for i in members):
            raise ValueError('Not a supported static 2D package')
        for item in members: check_2d(item.filename,z.read(item))
        out.mkdir(parents=True,exist_ok=False)
        archive=out/'archive.zip';archive.write_bytes(content)
        z.extractall(out/'members')
        receipt={'schema':'cad-agent.misumi-public-archive/v1','source_url':url,'observed_from':observed_from,'retrieved_at':datetime.now(timezone.utc).isoformat(),'sha256':actual,'bytes':len(content),'files':[{'path':i.filename,'bytes':i.file_size,'sha256':hashlib.sha256(z.read(i)).hexdigest()} for i in members], 'claim':'Static 2D file intake only; exact SKU geometry, 3D generation, NX import and purchase not verified'}
    (out/'receipt.json').write_text(json.dumps(receipt,ensure_ascii=False,indent=2)+'\n')
    return receipt

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--url',required=True);p.add_argument('--observed-from',required=True);p.add_argument('--out',required=True);p.add_argument('--expected-sha256');a=p.parse_args()
    try:
        receipt=download(a.url,a.observed_from,a.out,a.expected_sha256)
        print(json.dumps(receipt,ensure_ascii=False,indent=2));return 0
    except (OSError,ValueError,zipfile.BadZipFile) as exc: print(json.dumps({'error':str(exc)},ensure_ascii=False));return 1
if __name__=='__main__':raise SystemExit(main())
