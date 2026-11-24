import struct
from pathlib import Path
p=Path('/Volumes/SSD/WiiDev/WeeToolz/hbc-rs/assets/droid.ttf')
d=p.read_bytes(); n=struct.unpack_from('>H',d,4)[0]
tables={}
for i in range(n):
 tag,_,off,size=struct.unpack_from('>4sIII',d,12+i*16); tables[tag]=d[off:off+size]
c=tables[b'cmap']; count=struct.unpack_from('>H',c,2)[0]
for i in range(count):
 platform,enc,off=struct.unpack_from('>HHI',c,4+i*8)
 if struct.unpack_from('>H',c,off)[0]==4 and platform==3:
  break
else: raise RuntimeError('no cmap format 4')
def glyph(ch):
 seg=struct.unpack_from('>H',c,off+6)[0]//2; e=off+14; st=e+2*seg+2; delta=st+2*seg; ro=delta+2*seg
 for j in range(seg):
  end=struct.unpack_from('>H',c,e+2*j)[0]; start=struct.unpack_from('>H',c,st+2*j)[0]
  if start<=ch<=end:
   x=struct.unpack_from('>H',c,ro+2*j)[0]; v=struct.unpack_from('>h',c,delta+2*j)[0]
   if x:
    g=struct.unpack_from('>H',c,ro+2*j+x+2*(ch-start))[0]
    return (g+v)&65535 if g else 0
   return (ch+v)&65535
 raise ValueError(ch)
a,v=map(glyph,map(ord,'AV')); print('glyphs',a,v)
# Legacy kern v0, horizontal, one A/V pair at -200 font units.
pair=struct.pack('>HHh',a,v,-200)
tables[b'kern']=struct.pack('>HHHHHHHHHH',0,1,0,20,1,1,6,0,0,0)+pair
# The subtable is 14 bytes plus one pair; the file header is four bytes.
tables[b'kern']=struct.pack('>HHHHHHHH',0,1,0,20,1,1,6,0)+struct.pack('>H',0)+pair
# Rebuild the SFNT directory and checksums.
tags=sorted(tables); n=len(tags); power=2**(n.bit_length()-1)
header=struct.pack('>IHHHH',0x10000,n,power*16,power.bit_length()-1,n*16-power*16)
entries=[]; body=bytearray(); pos=12+n*16
for tag in tags:
 data=bytearray(tables[tag]);
 if tag==b'head': data[8:12]=b'\0'*4
 padded=data+b'\0'*((-len(data))%4); checksum=sum(struct.unpack('>%dI'%(len(padded)//4),padded))&0xffffffff
 entries.append(struct.pack('>4sIII',tag,checksum,pos,len(data))); body.extend(padded); pos+=len(padded)
out=bytearray(header+b''.join(entries)+body)
head_off=struct.unpack_from('>I',out,12+tags.index(b'head')*16+8)[0]
checksum=sum(struct.unpack('>%dI'%(len(out)//4),out))&0xffffffff
struct.pack_into('>I',out,head_off+8,(0xb1b0afba-checksum)&0xffffffff)
target=Path('/private/tmp/hbc-rs/kern-fixture/assets/droid.ttf');target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(out)
print('bytes',len(out))
