#!/usr/bin/env python3
"""Surgical frozen-runtime convergence. Preserves the encrypted original compose."""
import hashlib,json,os,subprocess,time
from pathlib import Path
baseline='sha256:619bc050cdec3cfe593b32a905c35a8612d8753aa861517ca3456e4bd5cbd912'
original='ghcr.io/ronitnath/ronitnath-app@sha256:b1cbe94cfcb43e500c61485f6ee3b6a7ba7c44962c633d229c34faa3e96b214a'
current=json.loads(subprocess.check_output(['docker','inspect','ronitnath-web']))[0]
assert current['Image']==baseline,'Verified native baseline changed; refuse deployment'
compose=Path('/data/crypt/ronitnath/compose.yaml');source=compose.read_text()
assert source.count(original)==1,'Unknown native compose image selector'
backup=compose.with_name('compose.before-subject-guard-'+str(int(time.time()))+'.yaml')
backup.write_text(source);os.chmod(backup,0o600)
new=source.replace(original,'ronitnath:workforce-subject-guard')
staged=compose.with_name('compose.subject-guard-candidate.yaml');staged.write_text(new);os.chmod(staged,0o600)
subprocess.run(['docker','compose','-f',str(staged),'config','--quiet'],check=True)
os.replace(staged,compose)
subprocess.run(['docker','compose','-f',str(compose),'up','-d','--no-deps','web'],check=True)
print(json.dumps({'backup':str(backup),'prior_sha256':hashlib.sha256(source.encode()).hexdigest(),'native_baseline':baseline,'image':subprocess.check_output(['docker','inspect','ronitnath-web','--format','{{.Image}}']).decode().strip()}))
