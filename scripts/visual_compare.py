"""Frozen RGBA8 normalized comparator; deterministic 11x11 Gaussian SSIM."""
import hashlib
from pathlib import Path
import numpy as np
from PIL import Image

VERSION='rgba8-normalized/2-gaussian11-sigma1.5'

def rgba(path):
    with Image.open(path) as image:
        if image.format!='PNG' or image.mode!='RGBA':
            raise ValueError('FormatMismatch: expected PNG RGBA8')
        return np.asarray(image).copy()

def gaussian(a):
    weights=np.exp(-(np.arange(-5,6,dtype=np.float64)**2)/(2*1.5**2))
    weights/=weights.sum()
    result=a
    for axis in (0,1):
        pad=[(0,0)]*a.ndim;pad[axis]=(5,5)
        padded=np.pad(result,pad,mode='reflect')
        result=sum(weight*np.take(padded,range(i,i+a.shape[axis]),axis=axis) for i,weight in enumerate(weights))
    return result

def ssim(a,b):
    if min(a.shape[:2])<11: return None
    m1,m2=gaussian(a),gaussian(b)
    v1=np.maximum(gaussian(a*a)-m1*m1,0);v2=np.maximum(gaussian(b*b)-m2*m2,0)
    cov=gaussian(a*b)-m1*m2
    values=((2*m1*m2+.01**2)*(2*cov+.03**2))/((m1*m1+m2*m2+.01**2)*(v1+v2+.03**2))
    return float(values[5:-5,5:-5].mean())

def compare(reference,output,config,diff_path):
    a,b=rgba(reference),rgba(output)
    if a.shape!=b.shape: raise ValueError('DimensionMismatch')
    delta=np.abs(a.astype(np.int16)-b.astype(np.int16))
    Image.fromarray(np.concatenate([np.minimum(delta[:,:,:3]*8,255).astype('uint8'),np.full((*a.shape[:2],1),255,dtype='uint8')],axis=2),'RGBA').save(diff_path)
    score=ssim(a.astype(np.float64)/255,b.astype(np.float64)/255)
    mae=float((delta/255).mean());fraction=float(np.any(delta>1,axis=2).mean())
    hashes=[hashlib.sha256(value.tobytes()).hexdigest() for value in (a,b)]
    metrics={'algorithm_version':VERSION,'ssim':score,'mae':mae,'diff_percent':fraction,'rgba_hashes':hashes,'dimensions':[a.shape[1],a.shape[0]]}
    if score is None and not config['allow_degraded_compare']:
        status='error'
    else:
        passed=mae<=config['mae_max'] and fraction<=config['diff_percent_max'] and (score is None or score>=config['ssim_min']) and (not config['exact_hash'] or hashes[0]==hashes[1])
        status=('degraded' if score is None else 'pass') if passed else 'regression'
    return dict(metrics,status=status)

def reference_valid(case,root):
    ref=case['reference'];path=Path(root)/ref['path']
    if ref['sha256'] is None:return 'reference-pending'
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest()!=ref['sha256']:return 'reference-invalid'
    pixels=rgba(path)
    if list(pixels.shape[:2])!=[ref['height'],ref['width']]:return 'reference-invalid'
    return 'valid'
