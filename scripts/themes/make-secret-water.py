# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (c) The Audiform contributors.
# This file is part of Audiform, based on Altered Audio Editor.
#
# Needs numpy, scipy and Pillow. Run from an empty folder: it writes the picture there.
# Procedural swirling painted-water scene for the secret theme (original artwork, generated here).
import numpy as np
from PIL import Image, ImageFilter
from scipy.ndimage import map_coordinates, gaussian_filter
W,H=1280,800
rng=np.random.default_rng(7)
def noise(scale,seed):
    r=np.random.default_rng(seed)
    gh,gw=int(H/scale)+3,int(W/scale)+3
    g=r.random((gh,gw)).astype(np.float32)
    yy,xx=np.mgrid[0:H,0:W].astype(np.float32)
    return map_coordinates(g,[yy/scale,xx/scale],order=3,mode='reflect')
def fbm(seed,base=260,oct=5):
    t=np.zeros((H,W),np.float32);amp=1;tot=0
    for o in range(oct):
        t+=amp*noise(base/(2**o),seed+o);tot+=amp;amp*=0.5
    return t/tot
yy,xx=np.mgrid[0:H,0:W].astype(np.float32)
def warp(f,dx,dy,k):
    return map_coordinates(f,[np.clip(yy+k*dy,0,H-1),np.clip(xx+k*dx,0,W-1)],order=3,mode='reflect')
base=fbm(1)
a=fbm(20)-0.5; b=fbm(30)-0.5
w1=warp(base,a,b,420)
c=fbm(40)-0.5; d=fbm(50)-0.5
w2=warp(w1,c,d,300)
# swirl ridges: the thin bright lines of paint folding over itself
ridge=1-np.abs(2*gaussian_filter(w2,1.2)-1)
ridge=ridge**6
t=(w2-w2.min())/(w2.max()-w2.min())
# palette: ink navy -> cobalt -> teal -> pale aqua, ridges push toward lime and white
stops=[(0.0,(5,8,28)),(0.28,(12,28,92)),(0.5,(24,72,160)),(0.7,(34,140,190)),(0.88,(110,200,205)),(1.0,(215,238,235))]
xs=[s[0] for s in stops]
img=np.stack([np.interp(t,xs,[s[1][i] for s in stops]) for i in range(3)],-1).astype(np.float32)
lime=np.array([196,228,70],np.float32); red=np.array([210,70,50],np.float32)
m2=gaussian_filter(np.clip(fbm(60)-0.55,0,1)*4,6)
img+= (ridge[...,None]*np.clip(m2*3,0,1)[...,None])*(lime-img)*0.9
img+= (ridge[...,None]*0.35)*(np.array([190,225,240],np.float32)-img)
m3=gaussian_filter(np.clip(fbm(70,200)-0.62,0,1)*6,5)
img+= (ridge[...,None]*np.clip(m3*2,0,1)[...,None])*(red-img)*0.7
# a dark heavy corner and a light pool, like the picture's composition
vy=(yy/H);vx=(xx/W)
vign=np.clip(0.55+0.7*(1-np.hypot(vx-0.55,vy-0.5)*1.3),0.35,1.15)[...,None]
img=np.clip(img*vign,0,255)
out=Image.fromarray(img.astype(np.uint8)).filter(ImageFilter.GaussianBlur(0.6))
out.save('water.webp','WEBP',quality=68,method=6)
out.save('water.png')
print(len(open('water.webp','rb').read()))
