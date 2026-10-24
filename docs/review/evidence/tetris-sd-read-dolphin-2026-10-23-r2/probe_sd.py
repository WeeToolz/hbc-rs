import argparse, hashlib, importlib.util, json, os, shutil, socket, subprocess, sys, tempfile, time
from pathlib import Path
ROOT=Path('/Volumes/SSD/WiiDev/RustPorts/hbc-rs')
sys.path.insert(0,str(ROOT/'tools'))
from dolphin_test_support import Debugger, write_dolphin_config
from test_channel_wad_dolphin import capture_screenshot
from test_hbc_ui_dolphin import set_initial_sort

def module(name,filename):
    spec=importlib.util.spec_from_file_location(name,ROOT/'tools'/filename)
    value=importlib.util.module_from_spec(spec); spec.loader.exec_module(value); return value
control=module('control','hbc-control.py'); inputs=module('inputs','hbc-input.py')
p=argparse.ArgumentParser(); p.add_argument('--package',type=Path,required=True); p.add_argument('--output',type=Path,required=True); a=p.parse_args()
a.output.mkdir(exist_ok=False)
manifest=json.loads((a.package/'manifest.json').read_text())
for name,identity in manifest['files'].items():
    source=a.package/name
    assert source.stat().st_size==identity['size'] and hashlib.sha256(source.read_bytes()).hexdigest()==identity['sha256']
wad=a.package/'hbc-rs-debug-OHBC.wad'; elf=a.package/'hbc-rs-debug.elf'
app=Path('/Volumes/SSD/WiiDev/Devtools/libogc-rs/apps/tetris')
data=(app/'boot.dol').read_bytes(); app_manifest=json.loads((app/'boot.manifest.json').read_text())
assert hashlib.sha256(data).hexdigest()==app_manifest['dol_sha256']
entry=int.from_bytes(data[0xe0:0xe4],'big')
entry_bytes=None
for i in range(7):
    off=int.from_bytes(data[4*i:4*i+4],'big'); addr=int.from_bytes(data[0x48+4*i:0x4c+4*i],'big'); size=int.from_bytes(data[0x90+4*i:0x94+4*i],'big')
    if addr<=entry and entry+32<=addr+size and off+size<=len(data): entry_bytes=data[off+entry-addr:off+entry-addr+32]
assert entry_bytes is not None
names={}
for line in subprocess.check_output(['/opt/devkitpro/devkitPPC/bin/powerpc-eabi-nm','-C',str(elf)],text=True).splitlines():
    fields=line.split(maxsplit=2)
    if len(fields)==3: names[fields[2]]=int(fields[0],16)
parse_address=names['hbc_rs::loader::parse_dol']
result={'success':False,'physical_wii_tested':False,'hbc_package':manifest,'app_manifest':app_manifest,'app_sha256':hashlib.sha256(data).hexdigest(),'app_bytes':len(data),'screenshots':[]}
host='192.168.1.52'
with socket.socket() as check: check.bind((host,4299))
process=connection=None; hotkey=None
with tempfile.TemporaryDirectory(prefix='hbc-tetris-sd-',dir='/Volumes/SSD/WiiDev/tmp') as temporary:
    profile=Path(temporary)
    try:
        shutil.copytree(Path.home()/'Library/Application Support/Dolphin/Wii',profile/'Wii')
        cfg=profile/'Config'; cfg.mkdir()
        write_dolphin_config(cfg,'[Core]\nWiiSDCard=True\nWiiSDCardAllowWrites=True\nWiiSDCardEnableFolderSync=True\nCPUThread=False\n[Interface]\nConfirmStop=False\nUsePanicHandlers=False\n[Input]\nBackgroundInput=True\n[General]\nHotkeysRequireFocus=False\n')
        (cfg/'Logger.ini').write_text('[Options]\nWriteToFile=True\nVerbosity=4\n[Logs]\nOSREPORT=True\nCORE=True\nBOOT=True\nIOS_ES=True\nIOS_NET=True\n')
        (cfg/'Hotkeys.ini').write_text('[Hotkeys]\nDevice = Pipe/0/priiloader-return\nGeneral/Take Screenshot = `Button Y`\n')
        set_initial_sort(profile)
        shutil.copytree(app,profile/'Load/WiiSDSync/apps/tetris')
        pipes=profile/'Pipes'; pipes.mkdir(); fifo=pipes/'priiloader-return'; os.mkfifo(fifo,0o600); hotkey=os.open(fifo,os.O_RDWR|os.O_NONBLOCK)
        console=(a.output/'console.log').open('w')
        port=55287
        process=subprocess.Popen(['/Applications/Dolphin.app/Contents/MacOS/Dolphin','-d','-u',str(profile),'-e',str(wad.resolve()),'-C',f'Dolphin.General.GDBPort={port}','-C','Dolphin.Core.CPUThread=False'],stdout=console,stderr=console)
        deadline=time.monotonic()+30
        while time.monotonic()<deadline:
            try: connection=socket.create_connection(('127.0.0.1',port),timeout=.5); break
            except OSError:
                if process.poll() is not None: raise RuntimeError('Dolphin exited before debugger')
                time.sleep(.1)
        if connection is None: raise TimeoutError('Debugger did not start')
        connection.settimeout(1); debug=Debugger(connection,names); result['initial_stop']=debug.command('?')
        debug.breakpoint(parse_address); debug.send('c')
        log=profile/'Logs/dolphin.log'
        deadline=time.monotonic()+90
        while time.monotonic()<deadline:
            text=log.read_text(errors='replace') if log.exists() else ''
            if 'HBC: storage scan applied: volume=Some(Sd), apps=1' in text:
                try: result['parent_ready']=control.exchange(host,4299,0,0,timeout=1); break
                except (OSError,ValueError): pass
            if process.poll() is not None: raise RuntimeError('Dolphin exited before catalog')
            time.sleep(.2)
        else: raise TimeoutError('Parent did not load one-app catalog')
        def button(name,release=True):
            if inputs.main(['--wii',host,'--kind','logical','--hold-ms','100',name])!=0: raise RuntimeError('Input failed: '+name)
            time.sleep(.15)
            if release and inputs.main(['--wii',host,'--kind','logical','--release'])!=0: raise RuntimeError('Release failed')
        def capture(stage):
            path=capture_screenshot(hotkey,profile/'ScreenShots',a.output)
            result['screenshots'].append({'stage':stage,'path':str(path)})
        button('a'); button('left'); capture('tetris-details')
        try: button('a',False)
        except (OSError,RuntimeError) as error: result['launch_input_reply_error']=str(error)
        print('Waiting for the production DOL parser after SD read', flush=True)
        result['parse_stop']=debug.stopped_at(parse_address,180)
        registers=bytes.fromhex(debug.command('g'))
        pointer=int.from_bytes(registers[4*4:5*4],'big'); length=int.from_bytes(registers[5*4:6*4],'big')
        result['parse_input']={'pointer':f'{pointer:08x}','bytes':length}
        assert length==len(data),result['parse_input']
        header=debug.memory(pointer,256); (a.output/'guest-header.bin').write_bytes(header); (a.output/'file-header.bin').write_bytes(data[:256])
        result['guest_header_matches_file']=header==data[:256]
        assert result['guest_header_matches_file'],'SD-read header differs from source'
        debug.breakpoint(parse_address,False); debug.breakpoint(entry); debug.send('c')
        print('Guest header captured; waiting for Tetris entry', flush=True)
        result['child_entry_stop']=debug.stopped_at(entry,60)
        result['child_entry_matches_file']=debug.memory(entry,32)==entry_bytes
        assert result['child_entry_matches_file'],'Child entry bytes differ from source'
        debug.breakpoint(entry,False); debug.send('c')
        time.sleep(8); capture('tetris-after-entry')
        result['child_runtime']='requires screenshot review'
        result['success']=True
    except Exception as error:
        result['error']=str(error)
    finally:
        if connection is not None: connection.close()
        if process is not None:
            process.terminate()
            try: process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill(); process.wait(timeout=5); result['success']=False; result['cleanup_error']='Dolphin required forced termination'
            result['dolphin_exit_code']=process.returncode
        if hotkey is not None: os.close(hotkey)
        log=profile/'Logs/dolphin.log'
        if log.exists(): shutil.copy2(log,a.output/'dolphin.log')
        shutil.copy2(__file__,a.output/'probe_sd.py')
        (a.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k not in ('hbc_package','app_manifest')},indent=2))
raise SystemExit(0 if result['success'] else 1)
