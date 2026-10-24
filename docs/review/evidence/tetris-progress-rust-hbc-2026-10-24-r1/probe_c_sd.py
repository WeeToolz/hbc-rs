import hashlib,json,os,shutil,socket,subprocess,sys,tempfile,time,threading
from pathlib import Path
ROOT=Path('/Volumes/SSD/WiiDev/RustPorts/hbc-rs'); sys.path.insert(0,str(ROOT/'tools'))
from dolphin_test_support import Debugger,write_dolphin_config
from dolphin_capture_config import configure_capture_profile
from test_channel_wad_dolphin import capture_screenshot
from test_hbc_ui_dolphin import RemotePipe,configure_profile,set_initial_sort
parent=Path('/private/tmp/hbc-rs-loading-current.dol')
app=Path('/private/tmp/tetris-reset-fixed-app'); data=(app/'boot.dol').read_bytes()
output=ROOT/'docs/review/evidence/tetris-progress-rust-hbc-2026-10-24-r1'; output.mkdir(exist_ok=False)
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
        write_dolphin_config(cfg,'[Core]\nWiiSDCard=False\nCPUThread=False\nSIDevice0=6\nSIDevice1=0\nSIDevice2=0\nSIDevice3=0\n[Interface]\nConfirmStop=False\nUsePanicHandlers=False\n[Input]\nBackgroundInput=True\n[General]\nHotkeysRequireFocus=False\n')
        configure_capture_profile(profile)
        set_initial_sort(profile);shutil.copytree(app,profile/'Load/WiiSDSync/apps/tetris')
        remote_fd=configure_profile(profile,cfg/'Dolphin.ini')
        (cfg/'GCPadNew.ini').write_text('[GCPad1]\nDevice = Pipe/0/hbc-ui-remote\nButtons/A = `Button A`\nButtons/B = `Button B`\nD-Pad/Down = `Button D_DOWN`\nD-Pad/Up = `Button D_UP`\nD-Pad/Left = `Button D_LEFT`\nD-Pad/Right = `Button D_RIGHT`\n')
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
            debug.send('c');result['parent_entry_stop']=debug.stopped_at(entry,180)
        debug.send('s');result['parent_step_stop']=debug.stopped(10);debug.send('c')
        remote=RemotePipe(remote_fd)
        def capture(stage):
            if process.poll() is not None:raise RuntimeError('Dolphin exited before '+stage)
            path=capture_screenshot(hotkey,profile/'ScreenShots',output)
            result['screenshots'].append({'stage':stage,'path':str(path)})
            print(stage,flush=True)
        time.sleep(10);remote.send('PRESS Z');capture('c-browser')
        remote.button('A');time.sleep(1);remote.button('D_LEFT');capture('rust-tetris-details')
        remote.button('A');print('Waiting for Rust HBC to enter Tetris',flush=True)
        def capture_loading_frames():
            for delay in (5, 20, 40):
                time.sleep(delay if delay == 5 else delay - (20 if delay == 40 else 5))
                if process.poll() is not None:
                    return
                capture(f'loading-{delay}s')
        capture_thread=threading.Thread(target=capture_loading_frames,daemon=True)
        capture_thread.start()
        result['child_entry_stop']=debug.stopped_at(entry,180)
        capture_thread.join(timeout=2)
        loaded=debug.memory(entry,32);(output/'child-entry.bin').write_bytes(loaded)
        result['child_code_matches_file']=loaded[:8]==expected[:8] and loaded[20:32]==expected[20:32]
        assert result['child_code_matches_file'],'Child startup code differs from Tetris'
        assert loaded[8:12]==b'_arg','Missing launch argv magic'
        ptr=int.from_bytes(loaded[12:16],'big');length=int.from_bytes(loaded[16:20],'big')
        assert 0<length<=1024,'Invalid launch argument length'
        args=debug.memory(ptr,length);result['launch_arguments_hex']=args.hex()
        assert args==b'sd:/apps/tetris/boot.dol\0\0','Unexpected app launch arguments'
        debug.breakpoint(entry,False);debug.breakpoint(0x80008024);debug.send('c')
        result['tetris_main_stop']=debug.stopped_at(0x80008024,90)
        debug.breakpoint(0x80008024,False)
        debug.breakpoint(0x80016958);debug.send('c')
        result['menu_entry']=debug.stopped_at(0x80016958,15)
        debug.breakpoint(0x80016958,False)
        lr=int(debug.command('p43'),16);debug.breakpoint(lr);debug.send('c')
        result['menu_draw_complete']=debug.stopped_at(lr,15);debug.breakpoint(lr,False)
        from xfb_capture import write_yuyv_png
        def summarize_yuyv_luma(data,width,height):
            if len(data)!=width*height*2:raise RuntimeError('Truncated framebuffer')
            return {'active_luma_pixels':sum(v>32 for v in data[::2]),'luma_levels':len(set(data[::2]))}
        def capture_xfb(label):
            pointer=int.from_bytes(debug.memory(0x809635b4,4),'big')
            frame=debug.memory((pointer&0x3fffffff)|0x80000000,640*480*2)
            write_yuyv_png(output/(label+'.png'),frame,640,480)
            result[label]=summarize_yuyv_luma(frame,640,480)
        capture_xfb('menu')
        debug.send('c');time.sleep(2)
        remote.button('A');time.sleep(3)
        connection.sendall(b'\x03');result['gameplay_stop']=debug.stopped(10)
        # Dolphin emits an extra empty packet after a debugger interrupt.
        debug.send('m809635b4,4');reply=debug.packet()
        while reply == '' or reply.startswith(('T','S')):reply=debug.packet()
        pointer=int.from_bytes(bytes.fromhex(reply),'big')
        frame=debug.memory((pointer&0x3fffffff)|0x80000000,640*480*2)
        write_yuyv_png(output/'gameplay.png',frame,640,480)
        result['gameplay']=summarize_yuyv_luma(frame,640,480)
        result['success']=True;result['visual_verification']='requires screenshot review'
    except Exception as error:
        result['error']=str(error)
        if connection is not None:
            try:
                connection.sendall(b'\x03'); result['failure_stop']=debug.stopped(10)
                import re
                sp=int(re.search(r'01:([0-9a-fA-F]+);',result['failure_stop']).group(1),16)
                result['frames']=[]
                for _ in range(24):
                    if not 0x80000000<=sp<0x81800000:break
                    debug.send(f'm{sp:x},10')
                    reply=debug.packet()
                    while reply == '' or reply.startswith(('T','S')):reply=debug.packet()
                    frame=bytes.fromhex(reply)
                    result['frames'].append({'sp':hex(sp),'data':frame.hex()})
                    next_sp=int.from_bytes(frame[:4],'big')
                    if next_sp<=sp:break
                    sp=next_sp
                for name,addr,size in [('main-stack',0x817e0000,0x20000),('thread-table',0x804d9f80,0x14000)]:
                    (output/(name+'.bin')).write_bytes(debug.memory(addr,size))

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
