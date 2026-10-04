#!/usr/bin/env python3
"""Surgical frozen-runtime convergence. Preserves the encrypted original compose."""
import hashlib,json,os,subprocess,time
from pathlib import Path
baseline='sha256:619bc050cdec3cfe593b32a905c35a8612d8753aa861517ca3456e4bd5cbd912'
original='${IMAGE:-ghcr.io/ronitnath/ronitnath-app:${TAG}}'
current=json.loads(subprocess.check_output(['docker','inspect','ronitnath-web']))[0]
assert current['Image']==baseline,'Verified native baseline changed; refuse deployment'
compose=Path('/data/crypt/ronitnath/compose.yaml');source=compose.read_text()
current_env=dict(entry.split('=',1) for entry in current['Config']['Env'])
runtime_env=dict(os.environ,TAG=current_env['APP_VERSION'])
assert source.count(original)==1,'Unknown native compose image selector'
backup=compose.with_name('compose.before-subject-guard-'+str(int(time.time()))+'.yaml')
backup.write_text(source);os.chmod(backup,0o600)
new=source.replace(original,'ronitnath:workforce-subject-guard')
staged=compose.with_name('compose.subject-guard-candidate.yaml');staged.write_text(new);os.chmod(staged,0o600)
configuration=json.loads(subprocess.check_output(['docker','compose','-f',str(staged),'config','--format','json'],env=runtime_env))
differences=[key for key,value in configuration['services']['web'].get('environment',{}).items() if value!=current_env.get(key)]
assert not differences,'Unexpected native environment differences: '+','.join(differences)
# TAG is public deployment metadata; persist it without editing native secret files.
tag_file=compose.with_name('workforce-guard.env');tag_file.write_text('TAG='+current_env['APP_VERSION']+'\n');os.chmod(tag_file,0o600)
os.replace(staged,compose)
subprocess.run(['docker','compose','--env-file',str(tag_file),'-f',str(compose),'up','-d','--no-deps','web'],check=True)
print(json.dumps({'backup':str(backup),'prior_sha256':hashlib.sha256(source.encode()).hexdigest(),'native_baseline':baseline,'image':subprocess.check_output(['docker','inspect','ronitnath-web','--format','{{.Image}}']).decode().strip()}))
