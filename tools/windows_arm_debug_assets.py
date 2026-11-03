#!/usr/bin/env python3
"""Prepare local Minecraft 26.3 resources for Horizon Windows ARM64 debug.

Downloads only from Mojang's official HTTPS endpoints and verifies SHA-1,
the version manifest against research-lock.json, and all asset object sizes.
Never packages or commits Minecraft-owned files.
"""
from __future__ import annotations
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import threading
import urllib.parse
import urllib.request
from zipfile import ZipFile

ROOT=Path(__file__).resolve().parents[1]
LOCK=json.loads((ROOT/"research-lock.json").read_text(encoding="utf-8"))["minecraft"]
VERSION=LOCK["version"]
CACHE=ROOT/".local"/"downloads"/VERSION
DEST=ROOT/".local"/"horizon-client"/VERSION
HOSTS={"piston-meta.mojang.com","piston-data.mojang.com",
       "resources.download.minecraft.net","launcher.mojang.com",
       "launchermeta.mojang.com"}
CACHE.mkdir(parents=True,exist_ok=True)
DEST.mkdir(parents=True,exist_ok=True)
print(f"Mojang-verified resources for Horizon {VERSION}",flush=True)

def fetch_bytes(url,limit=16_777_216):
    u=urllib.parse.urlparse(url)
    if u.scheme!="https" or u.hostname not in HOSTS or u.username or u.password or u.port not in (None,443):
        raise ValueError(f"Non-Mojang URL forbidden: {url}")
    req=urllib.request.Request(url,headers={"User-Agent":"HorizonDebugResourceVerifier/1.0"})
    with urllib.request.urlopen(req,timeout=55) as response:
        data=response.read(limit+1)
    if len(data)>limit:
        raise ValueError("Mojang metadata exceeds size limit")
    return data

def download(url,size,sha,path):
    path=Path(path)
    if path.is_file() and path.stat().st_size==size:
        if hashlib.sha1(path.read_bytes()).hexdigest()==sha:
            return path
    path.parent.mkdir(parents=True,exist_ok=True)
    data=fetch_bytes(url,max(size,1))
    if len(data)!=size or hashlib.sha1(data).hexdigest()!=sha:
        raise ValueError(f"SHA-1 or size mismatch: {path.name}")
    partial=path.with_suffix(path.suffix+".partial")
    partial.write_bytes(data)
    partial.replace(path)
    return path

catalog=json.loads(fetch_bytes("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"))
entry=next(x for x in catalog["versions"] if x["id"]==VERSION)
accepted={LOCK["manifest_sha1"],*LOCK.get("accepted_manifest_sha1",[])}
if entry["sha1"] not in accepted:
    raise ValueError("Unrecognized Mojang version manifest hash")
data=fetch_bytes(entry["url"])
if hashlib.sha1(data).hexdigest()!=entry["sha1"]:
    raise ValueError("Mojang metadata checksum invalid")
meta=json.loads(data)
client=meta["downloads"]["client"]
if meta["id"]!=VERSION or client["sha1"]!=LOCK["client_sha1"]:
    raise ValueError("Mojang client hash differs from Horizon research lock")
jar=download(client["url"],client["size"],client["sha1"],CACHE/"client.jar")
print(f"Verified official client jar: {jar.stat().st_size/1_048_576:.1f} MiB",flush=True)

# All asset/data paths in Mojang's client jar (not Java classes).
with ZipFile(jar) as z:
    entries=[entry for entry in z.infolist()
             if not entry.is_dir()
             and (entry.filename.startswith("assets/") or entry.filename.startswith("data/"))]
    for entry in entries:
        rel=Path(entry.filename)
        if rel.is_absolute() or ".." in rel.parts or "\\" in entry.filename:
            raise ValueError("Unsafe Mojang client resource path")
        dst=DEST/rel
        dst.parent.mkdir(parents=True,exist_ok=True)
        with z.open(entry) as src, open(dst,"wb") as out:
            shutil.copyfileobj(src,out)
print(f"Installed {len(entries)} actual client asset/data resources",flush=True)

index_info=meta["assetIndex"]
index_path=download(index_info["url"],index_info["size"],index_info["sha1"],CACHE/"asset-index.json")
index=json.loads(index_path.read_text())
(DEST/"asset-index.json").write_bytes(index_path.read_bytes())
objects=[]
for name,obj in index["objects"].items():
    if name.startswith("minecraft/sounds/") or name=="minecraft/sounds.json":
        continue
    rel=Path(name)
    if rel.is_absolute() or ".." in rel.parts or "\\" in name:
        raise ValueError(f"Unsafe asset index path: {name}")
    h=obj["hash"]
    if len(h)!=40:
        raise ValueError("Bad Mojang object SHA-1")
    objects.append((name,obj))
print(f"Asset index: {len(objects)} non-audio objects",flush=True)

counter=0
lock=threading.Lock()
def one(row):
    global counter
    name,obj=row
    h=obj["hash"]
    cache=CACHE/"objects"/h
    download(f"https://resources.download.minecraft.net/{h[:2]}/{h}",obj["size"],h,cache)
    dst=DEST/"assets"/name
    dst.parent.mkdir(parents=True,exist_ok=True)
    shutil.copyfile(cache,dst)
    with lock:
        counter+=1
        if counter%250==0 or counter==len(objects):
            print(f"Verified Mojang objects {counter}/{len(objects)}",flush=True)

with concurrent.futures.ThreadPoolExecutor(max_workers=10) as pool:
    list(pool.map(one,objects))
required=(DEST/"assets/minecraft/textures/gui/title/background/panorama_1.png",
          DEST/"horizon-resources.json",DEST/"asset-index.json")
for item in required:
    if not item.is_file():
        raise RuntimeError(f"Required resource absent: {item}")
print("READY: All debug resources installed from verified Mojang inputs",flush=True)
