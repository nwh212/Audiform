# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (c) The Audiform contributors.
# This file is part of Audiform, based on Altered Audio Editor.
#
# Needs numpy, scipy and Pillow. Run from an empty folder: it writes the picture there.

# The wavy AUDIFORM title for the secret theme: tall condensed lettering drawn here, waved like paint in water.
import numpy as np
from PIL import Image, ImageDraw, ImageFont, ImageFilter
S=4; FH=150  # draw at 4x
font=ImageFont.truetype("Inter-Black.otf", FH*S//2*2//2)
text="AUDIFORM"
tmp=Image.new("L",(2400,700),0)
d=ImageDraw.Draw(tmp); d.text((30,20),text,font=font,fill=255)
bb=tmp.getbbox(); m=tmp.crop(bb)
# condense: tall narrow letters
m=m.resize((int(m.width*0.62),m.height))
pad=40
canvas=Image.new("L",(m.width+2*pad,m.height+2*pad),0); canvas.paste(m,(pad,pad))
a=np.asarray(canvas).astype(np.float32)
H,W=a.shape
yy,xx=np.mgrid[0:H,0:W].astype(np.float32)
# wave: the lettering ripples up and down, and sways a little sideways
dy=14*np.sin(xx/ (W/9.0)*2*np.pi*0.8+0.6)+5*np.sin(xx/(W/23.0)*2*np.pi+1.3)
dx=7*np.sin(yy/(H/3.0)*2*np.pi+0.2)
from scipy.ndimage import map_coordinates
a=map_coordinates(a,[np.clip(yy+dy,0,H-1),np.clip(xx+dx,0,W-1)],order=3)
mask=Image.fromarray(np.clip(a,0,255).astype(np.uint8))
# outline and soft shadow
edge=mask.filter(ImageFilter.MaxFilter(9))
shadow=edge.filter(ImageFilter.GaussianBlur(7))
out=np.zeros((H,W,4),np.float32)
sh=np.asarray(shadow).astype(np.float32)/255
out[...,:3]=np.array([4,10,40]);out[...,3]=sh*0.75*255
ed=np.asarray(edge).astype(np.float32)/255
ink=np.array([8,22,70],np.float32)
out[...,:3]=out[...,:3]*(1-ed[...,None])+ink*ed[...,None]
out[...,3]=np.maximum(out[...,3],ed*255)
f=np.asarray(mask).astype(np.float32)/255
# letter fill: cool, translucent water-white with a cobalt base and lime glint along the top
g=np.clip(yy/H,0,1)
top=np.array([205,238,240],np.float32); mid=np.array([120,190,225],np.float32); bot=np.array([70,130,200],np.float32)
fill=np.where(g[...,None]<0.5, top+(mid-top)*(g[...,None]/0.5), mid+(bot-mid)*((g[...,None]-0.5)/0.5))
glint=np.clip(np.sin(xx/9+yy/6)*0.5+0.5,0,1)*np.clip(1-g*3.2,0,1)
fill=fill+glint[...,None]*(np.array([214,236,90],np.float32)-fill)*0.45
out[...,:3]=out[...,:3]*(1-f[...,None])+fill*f[...,None]
out[...,3]=np.maximum(out[...,3],f*255)
img=Image.fromarray(np.clip(out,0,255).astype(np.uint8),"RGBA")
img=img.resize((img.width//S*1, img.height//S*1),Image.LANCZOS) if False else img.resize((int(img.width/2.4),int(img.height/2.4)),Image.LANCZOS)
img.save("title.webp","WEBP",quality=90,method=6,exact=True)
img.save("title.png")
print(img.size, len(open("title.webp","rb").read()))
