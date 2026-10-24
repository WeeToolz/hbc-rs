import hashlib,json,os,shutil,socket,subprocess,sys,tempfile,time
from pathlib import Path
ROOT=Path('/Volumes/SSD/WiiDev/RustPorts/hbc-rs'); sys.path.insert(0,str(ROOT/'tools'))
from dolphin_test_support import Debugger,write_dolphin_config
from dolphin_capture_config import configure_capture_profile
from test_channel_wad_dolphin import capture_screenshot
from test_hbc_ui_dolphin import RemotePipe,configure_profile,set_initial_sort
parent=Path('/Volumes/SSD/WiiDev/C-sources/hbc-reborn/channel/channelapp/channelapp-channel.dol')
app=Path('/private/tmp/tetris-current-default-app'); data=(app/'boot.dol').read_bytes()
output=ROOT/'docs/review/evidence/tetris-startup-trace-2026-10-23-r1'; output.mkdir(exist_ok=False)
entry=int.from_bytes(data[0xe0:0xe4],'big'); expected=None
for i in range(7):
    off=int.from_bytes(data[4*i:4*i+4],'big'); addr=int.from_bytes(data[0x48+4*i:0x4c+4*i],'big'); length=int.from_bytes(data[0x90+4*i:0x94+4*i],'big')
    if addr<=entry and entry+32<=addr+length and off+length<=len(data): expected=data[off+entry-addr:off+entry-addr+32]
assert expected is not None
result={'success':False,'physical_wii_tested':False,'parent':str(parent),'parent_sha256':hashlib.sha256(parent.read_bytes()).hexdigest(),'app':str(app/'boot.dol'),'app_sha256':hashlib.sha256(data).hexdigest(),'app_bytes':len(data),'input_path':'Dolphin Pipe emulated Wii Remote','screenshots':[]}
process=connection=None; remote_fd=hotkey=None
with tempfile.TemporaryDirectory(prefix='hbc-c-tetris-sd-',dir='/private/tmp') as temporary:
    profile=Path(temporary)
    try:
        shutil.copytree(Path.home()/'Library/Application Support/Dolphin/Wii',profile/'Wii')
        cfg=profile/'Config';cfg.mkdir()
        write_dolphin_config(cfg,'[Core]\nWiiSDCard=False\nCPUThread=False\n[Interface]\nConfirmStop=False\nUsePanicHandlers=False\n[Input]\nBackgroundInput=True\n[General]\nHotkeysRequireFocus=False\n')
        configure_capture_profile(profile)
        set_initial_sort(profile);shutil.copytree(app,profile/'Load/WiiSDSync/apps/tetris')
        remote_fd=configure_profile(profile,cfg/'Dolphin.ini')
        with (cfg/'WiimoteNew.ini').open('a') as f: f.write('IR/Hide = `Pipe/0/hbc-ui-remote:Button Z`\n')
        (cfg/'Logger.ini').write_text('[Options]\nWriteToFile=True\nVerbosity=4\n[Logs]\nOSREPORT=True\nCORE=True\nBOOT=True\nIOS_ES=True\nIOS_NET=True\n')
        (cfg/'Hotkeys.ini').write_text('[Hotkeys]\nDevice = Pipe/0/priiloader-return\nGeneral/Take Screenshot = `Button Y`\n')
        fifo=profile/'Pipes/priiloader-return';os.mkfifo(fifo,0o600);hotkey=os.open(fifo,os.O_RDWR|os.O_NONBLOCK)
        shutil.copytree(cfg,output/'Config')
        console=(output/'console.log').open('w')
        process=subprocess.Popen(['/Applications/Dolphin.app/Contents/MacOS/Dolphin','-d','-u',str(profile),'-e',str(parent),'-C','Dolphin.General.GDBPort=55287','-C','Dolphin.Core.CPUThread=False'],stdout=console,stderr=console)
        deadline=time.monotonic()+30
        while time.monotonic()<deadline:
            try: connection=socket.create_connection(('127.0.0.1',55287),timeout=.5);break
            except OSError:
                if process.poll() is not None:raise RuntimeError('Dolphin exited before debugger')
                time.sleep(.1)
        if connection is None:raise TimeoutError('Debugger did not start')
        debug=Debugger(connection,{});result['initial_stop']=debug.command('?')
        parent_entry=int.from_bytes(parent.read_bytes()[0xe0:0xe4],'big')
        assert parent_entry==entry,'This runner expects a shared entry address for the parent/child step check'
        debug.breakpoint(entry)
        if f'40:{entry:08x};' in result['initial_stop']:
            result['parent_entry_stop']=result['initial_stop']
        else:
            debug.send('c');result['parent_entry_stop']=debug.stopped_at(entry,30)
        debug.send('s');result['parent_step_stop']=debug.stopped(10);debug.send('c')
        remote=RemotePipe(remote_fd)
        def capture(stage):
            if process.poll() is not None:raise RuntimeError('Dolphin exited before '+stage)
            path=capture_screenshot(hotkey,profile/'ScreenShots',output)
            result['screenshots'].append({'stage':stage,'path':str(path)})
            print(stage,flush=True)
        time.sleep(10);remote.send('PRESS Z');capture('c-browser')
        remote.button('D_DOWN');time.sleep(.3);remote.button('A');time.sleep(1);capture('c-tetris-details')
        remote.button('A');print('Waiting for C Reborn to enter Tetris',flush=True)
        result['child_entry_stop']=debug.stopped_at(entry,180)
        loaded=debug.memory(entry,32);(output/'child-entry.bin').write_bytes(loaded)
        result['child_code_matches_file']=loaded[:8]==expected[:8] and loaded[20:32]==expected[20:32]
        assert result['child_code_matches_file'],'Child startup code differs from Tetris'
        assert loaded[8:12]==b'_arg','Missing launch argv magic'
        ptr=int.from_bytes(loaded[12:16],'big');length=int.from_bytes(loaded[16:20],'big')
        assert 0<length<=1024,'Invalid launch argument length'
        args=debug.memory(ptr,length);result['launch_arguments_hex']=args.hex()
        assert args==b'sd:/apps/tetris/boot.dol\0\0','Unexpected app launch arguments'
        debug.breakpoint(entry,False);debug.breakpoint(0x80008000);debug.send('c')
        result['tetris_main_stop']=debug.stopped_at(0x80008000,90)
        debug.breakpoint(0x80008000,False)
        result['checkpoints']=[]
        for label,address in [('system-ready',0x80008010),('argv-ready',0x80008014),('display-ready',0x80008018),('controllers-ready',0x80008044),('menu-draw',0x800172f8)]:
            debug.breakpoint(address);debug.send('c')
            stop=debug.stopped_at(address,15)
            record={'stage':label,'stop':stop,'r3':debug.command('p3')}
            result['checkpoints'].append(record);print(json.dumps(record),flush=True)
            debug.breakpoint(address,False)
        debug.send('c');time.sleep(2);capture('tetris-after-c-entry')
        result['success']=True;result['visual_verification']='requires screenshot review'
    except Exception as error:
        result['error']=str(error)
        if connection is not None:
            try:
                connection.sendall(b'\x03'); result['failure_stop']=debug.stopped(10); result['registers']=debug.command('g'); sp=int.from_bytes(bytes.fromhex(debug.command('p1')),'big');result['stack_address']=hex(sp);result['stack']=debug.memory(sp,512).hex()
            except Exception as inspection: result['failure_inspection_error']=str(inspection)
    finally:
        if connection is not None:connection.close()
        if process is not None:
            process.terminate()
            try:process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill();process.wait(timeout=5);result['success']=False;result['cleanup_error']='Dolphin required forced termination'
            result['dolphin_exit_code']=process.returncode
        for fd in (remote_fd,hotkey):
            if fd is not None:os.close(fd)
        log=profile/'Logs/dolphin.log'
        if log.exists():shutil.copy2(log,output/'dolphin.log')
        shutil.copy2(__file__,output/'probe_c_sd.py')
        (output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2));raise SystemExit(0 if result['success'] else 1)
