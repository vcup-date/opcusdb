// opcusdb Overlode — Three.js client for the authoritative Tracer FPS server.
// Thin client: predicts local movement for responsiveness, renders the world the
// server broadcasts, and sends inputs. Server does lag-compensated hit detection.

const $ = (id) => document.getElementById(id);
const ARENA = 28.0, EYE = 1.55, MOVE = 7.0, GRAVITY = 24.0, JUMP = 8.2, BLINK_DIST = 7.0;
const COVER = [ // must match the server
  [0,0,2,2,2.4],[10,8,1.5,1.5,2.0],[-10,8,1.5,1.5,2.0],[10,-8,1.5,1.5,2.0],
  [-10,-8,1.5,1.5,2.0],[0,14,4,1,3.0],[0,-14,4,1,3.0],
];
const TEAMCOL = [0x5aa0ff, 0xff8a4a];
const clamp = (v,a,b)=>Math.max(a,Math.min(b,v));
const shot = new URLSearchParams(location.search).has("shot");

let ws=null, myId=0, started=false;
let players=new Map();   // id -> {grp, plate, alive, team, name, tx,ty,tz,tyaw, elims,deaths}
let scoreA=0, scoreB=0;
let pos={x:0,y:0,z:22}, vel={x:0,y:0,z:0}, yaw=Math.PI, pitch=0, onGround=true;
let keys={w:false,s:false,a:false,d:false,jump:false};
let feed=[];
let latMs=0, prevHp=150, prevReload=false;

// ---- audio (Web Audio, procedural) ----------------------------------------
let actx=null;
function ac(){ if(!actx){ try{ actx=new (window.AudioContext||window.webkitAudioContext)(); }catch(e){} } return actx; }
function tone(f0,f1,d,type,v){ const c=ac(); if(!c)return; const o=c.createOscillator(),g=c.createGain();
  o.type=type; o.frequency.setValueAtTime(f0,c.currentTime); o.frequency.exponentialRampToValueAtTime(Math.max(1,f1),c.currentTime+d);
  g.gain.setValueAtTime(v,c.currentTime); g.gain.exponentialRampToValueAtTime(0.0001,c.currentTime+d); o.connect(g).connect(c.destination); o.start(); o.stop(c.currentTime+d); }
function noise(d,v,hp){ const c=ac(); if(!c)return; const n=c.sampleRate*d|0,b=c.createBuffer(1,n,c.sampleRate),dt=b.getChannelData(0);
  for(let i=0;i<n;i++)dt[i]=(Math.random()*2-1)*(1-i/n); const s=c.createBufferSource();s.buffer=b;
  const g=c.createGain();g.gain.value=v; const f=c.createBiquadFilter();f.type="highpass";f.frequency.value=hp; s.connect(f).connect(g).connect(c.destination); s.start(); }
const sfxFire=(v=0.12)=>{ tone(880,300,0.05,"square",v); noise(0.035,v*0.5,1600); };
const sfxBlink=()=>tone(300,1300,0.18,"sawtooth",0.12);
const sfxRecall=()=>tone(1300,200,0.45,"sine",0.16);
const sfxReload=()=>{ tone(220,420,0.07,"square",0.08); setTimeout(()=>tone(420,220,0.07,"square",0.08),150); };
const sfxHit=()=>tone(1700,1700,0.035,"square",0.11);
const sfxKill=()=>{ tone(700,1400,0.12,"triangle",0.18); setTimeout(()=>tone(1100,1500,0.1,"triangle",0.14),90); };
const sfxHurt=()=>noise(0.18,0.22,450);
const sfxHeal=()=>tone(520,920,0.2,"sine",0.13);
const sfxUlt=()=>{ tone(180,520,0.3,"sawtooth",0.16); };
const sfxBoom=()=>{ noise(0.5,0.32,180); tone(120,40,0.5,"sawtooth",0.22); };

// floating damage numbers (projected from a world point)
function dmgNumber(x,y,z,amt){
  const v=new THREE.Vector3(x,y,z); v.project(camera);
  if(v.z>1) return;
  const sx=(v.x*0.5+0.5)*innerWidth, sy=(-v.y*0.5+0.5)*innerHeight;
  const d=document.createElement("div"); d.textContent=Math.round(amt);
  d.style.cssText=`position:fixed;left:${sx}px;top:${sy}px;z-index:5;color:${amt>=50?"#ff6a4a":"#ffe066"};font:800 ${amt>=50?30:21}px ui-monospace,monospace;text-shadow:0 2px 5px #000;pointer-events:none;transition:transform .6s ease-out,opacity .6s`;
  document.body.appendChild(d);
  requestAnimationFrame(()=>{ d.style.transform="translateY(-42px)"; d.style.opacity="0"; });
  setTimeout(()=>d.remove(),650);
}

// ---- damage vignette ------------------------------------------------------
const vig=document.createElement("div");
vig.style.cssText="position:fixed;inset:0;z-index:3;pointer-events:none;box-shadow:inset 0 0 200px 40px rgba(255,30,30,0);transition:box-shadow .12s";
document.body.appendChild(vig);
function flashDmg(){ vig.style.boxShadow="inset 0 0 150px 44px rgba(255,30,30,0.42)"; setTimeout(()=>vig.style.boxShadow="inset 0 0 150px 40px rgba(255,30,30,0)",100); }

// ---- three.js setup -------------------------------------------------------
const renderer = new THREE.WebGLRenderer({antialias:true});
renderer.setSize(innerWidth, innerHeight);
renderer.setPixelRatio(Math.min(devicePixelRatio,2));
renderer.shadowMap.enabled = true;
$("app").appendChild(renderer.domElement);
const scene = new THREE.Scene();
scene.background = new THREE.Color(0x9fc0e8);
scene.fog = new THREE.Fog(0x9fc0e8, 40, 95);
const camera = new THREE.PerspectiveCamera(92, innerWidth/innerHeight, 0.05, 400);
camera.rotation.order = "YXZ";
scene.add(camera);
addEventListener("resize", ()=>{ camera.aspect=innerWidth/innerHeight; camera.updateProjectionMatrix(); renderer.setSize(innerWidth,innerHeight); });

scene.add(new THREE.HemisphereLight(0xdaeaff, 0x40503a, 1.15));
const sun = new THREE.DirectionalLight(0xfff4e0, 1.25);
sun.position.set(20,40,12); sun.castShadow=true;
sun.shadow.mapSize.set(1024,1024); sun.shadow.camera.left=-40; sun.shadow.camera.right=40; sun.shadow.camera.top=40; sun.shadow.camera.bottom=-40;
scene.add(sun);

const floor = new THREE.Mesh(new THREE.PlaneGeometry(80,80), new THREE.MeshStandardMaterial({color:0x4a5d3a}));
floor.rotation.x = -Math.PI/2; floor.receiveShadow=true; scene.add(floor);
// checker accent
const grid = new THREE.GridHelper(80, 40, 0x33402a, 0x33402a); grid.position.y=0.02; scene.add(grid);

function mat(c){ return new THREE.MeshStandardMaterial({color:c, roughness:0.9}); }
// arena walls
const wallMat = mat(0x3d4d6e);
for (const [x,z,w,d] of [[0,ARENA+0.5,ARENA*2+2,1],[0,-ARENA-0.5,ARENA*2+2,1],[ARENA+0.5,0,1,ARENA*2+2],[-ARENA-0.5,0,1,ARENA*2+2]]) {
  const wall = new THREE.Mesh(new THREE.BoxGeometry(w,4,d), wallMat); wall.position.set(x,2,z); wall.castShadow=true; wall.receiveShadow=true; scene.add(wall);
}
// cover
for (const [cx,cz,hx,hz,h] of COVER) {
  const box = new THREE.Mesh(new THREE.BoxGeometry(hx*2,h,hz*2), mat(0x7a6a52));
  box.position.set(cx,h/2,cz); box.castShadow=true; box.receiveShadow=true; scene.add(box);
}
// team spawn markers
for (const [z,c] of [[22,0x2a4a7a],[-22,0x7a3a1a]]) {
  const pad = new THREE.Mesh(new THREE.CircleGeometry(6,24), new THREE.MeshStandardMaterial({color:c}));
  pad.rotation.x=-Math.PI/2; pad.position.set(0,0.03,z); scene.add(pad);
}

// health packs (positions must match the server)
const PACK_POS=[[15,0],[-15,0],[0,18],[0,-18]];
const packMeshes=[]; let packAvail=[true,true,true,true];
for(const [px,pz] of PACK_POS){
  const g=new THREE.Group();
  g.add(new THREE.Mesh(new THREE.BoxGeometry(0.9,0.9,0.9), new THREE.MeshStandardMaterial({color:0xf2f2f2,emissive:0x0c2a14})));
  g.add(new THREE.Mesh(new THREE.BoxGeometry(0.58,0.2,0.5), new THREE.MeshBasicMaterial({color:0x2ee06a})));
  g.add(new THREE.Mesh(new THREE.BoxGeometry(0.2,0.58,0.5), new THREE.MeshBasicMaterial({color:0x2ee06a})));
  g.position.set(px,1.0,pz); scene.add(g); packMeshes.push(g);
}

// first-person weapon viewmodel (Tracer's pulse pistols)
const gun = new THREE.Group();
for (const sx of [-0.28, 0.28]) {
  const g = new THREE.Mesh(new THREE.BoxGeometry(0.16,0.16,0.5), mat(0x3a4150));
  g.position.set(sx,-0.32,-0.6); gun.add(g);
  const tip = new THREE.Mesh(new THREE.BoxGeometry(0.1,0.1,0.18), mat(0x6fe0ff));
  tip.position.set(sx,-0.32,-0.9); gun.add(tip);
}
camera.add(gun);
let recoil=0;
const muzzle = new THREE.Mesh(new THREE.PlaneGeometry(0.55,0.55), new THREE.MeshBasicMaterial({color:0xfff0a0,transparent:true,opacity:0,depthTest:false}));
muzzle.position.set(0,-0.32,-1.0); gun.add(muzzle);
let muzzleT=0; const flashMuzzle=()=>{ muzzleT=0.05; };

const tracers=[]; // {line, life}
const sparks=[];  // {mesh, life, vel}
const bombs3=[];  // pulse-bomb meshes (pooled)
let shakeT=0;
function updateBombs3(list){
  while(bombs3.length<list.length){ const m=new THREE.Mesh(new THREE.SphereGeometry(0.32,12,10), new THREE.MeshBasicMaterial({color:0x66e0ff})); scene.add(m); bombs3.push(m); }
  for(let i=0;i<bombs3.length;i++){ const on=i<list.length; bombs3[i].visible=on; if(on) bombs3[i].position.set(list[i][0],list[i][1],list[i][2]); }
}
function bigBoom(x,y,z){
  addSparks(x,y,z,0xffd24a,28);
  const m=new THREE.Mesh(new THREE.SphereGeometry(1,16,12), new THREE.MeshBasicMaterial({color:0x66e0ff,transparent:true,opacity:0.85}));
  m.position.set(x,y,z); scene.add(m); sparks.push({mesh:m,life:1,vel:null,ring:true});
  shakeT=0.5; sfxBoom();
}

// ---- networking -----------------------------------------------------------
function connect(nick) {
  ws = new WebSocket(`ws://${location.host}/ws`);
  ws.onopen = ()=>{ ws.send("join "+nick); setInterval(()=>{ if(ws.readyState===1) ws.send("ping "+performance.now().toFixed(0)); }, 1000); };
  ws.onmessage = (e)=>{
    const seen=new Set();
    for (const line of e.data.split("\n")) {
      if (!line) continue;
      const p = line.split("\t");
      if (p[0]==="w") myId=+p[1];
      else if (p[0]==="g") { scoreA=+p[2]; scoreB=+p[3]; }
      else if (p[0]==="p") { handlePlayer(p); seen.add(+p[1]); }
      else if (p[0]==="d") { packAvail = (p[1]||"").split(" ").map(v=>v==="1"); }
      else if (p[0]==="z") { updateBombs3((p[1]||"").split(";").filter(Boolean).map(s=>s.split(":").map(Number))); }
      else if (p[0]==="P") { latMs = Math.min(300, (performance.now()-(+p[1]))/2); }
      else if (p[0]==="x") { for (const ev of (p[1]||"").split(";")) if(ev) handleEvent(ev); }
    }
    for (const id of [...players.keys()]) if(!seen.has(id)) { scene.remove(players.get(id).grp); players.delete(id); }
    updateHUD();
  };
}

function handlePlayer(p) {
  const id=+p[1];
  const o = { x:+p[2],y:+p[3],z:+p[4], yaw:+p[5], pitch:+p[6], hp:+p[7], team:+p[8], alive:p[9]==="1",
    ammo:+p[10], blink:+p[11], blinkcd:+p[12], recallcd:+p[13], reload:p[14]==="1", elims:+p[15], deaths:+p[16], name:p[17], ult:+p[18] };
  if (id===myId) {
    me = o;
    // soft reconciliation toward the authoritative position
    const ex=o.x-pos.x, ey=o.y-pos.y, ez=o.z-pos.z;
    const err=Math.hypot(ex,ey,ez);
    if (err>2.5) { pos.x=o.x; pos.y=o.y; pos.z=o.z; }   // snap (blink/recall/respawn)
    else { pos.x+=ex*0.18; pos.y+=ey*0.5; pos.z+=ez*0.18; }
    return;
  }
  let e = players.get(id);
  if (!e) { e = spawnAvatar(id, o.team, o.name); players.set(id, e); }
  e.tx=o.x; e.ty=o.y; e.tz=o.z; e.tyaw=o.yaw; e.team=o.team; e.name=o.name;
  e.elims=o.elims; e.deaths=o.deaths;
  e.grp.visible = o.alive;
}
let me=null;

function spawnAvatar(id, team, name) {
  const grp = new THREE.Group();
  const body = new THREE.Mesh(new THREE.CapsuleGeometry(0.42,0.9,4,10), new THREE.MeshStandardMaterial({color:TEAMCOL[team]}));
  body.position.y=0.92; body.castShadow=true; grp.add(body);
  const visor = new THREE.Mesh(new THREE.BoxGeometry(0.5,0.18,0.12), mat(0xfff2c0));
  visor.position.set(0,1.45,-0.34); grp.add(visor);
  const plate = makePlate(name, team);
  plate.position.y=2.25; grp.add(plate);
  scene.add(grp);
  return { grp, plate, team, name, tx:0,ty:0,tz:0,tyaw:0, elims:0, deaths:0 };
}
function makePlate(name, team) {
  const c=document.createElement("canvas"); c.width=256; c.height=64;
  const g=c.getContext("2d"); g.font="bold 30px monospace"; g.textAlign="center";
  g.fillStyle="rgba(0,0,0,.5)"; g.fillRect(0,0,256,64);
  g.fillStyle = team===0 ? "#9fd0ff":"#ffc0a0"; g.fillText(name,128,42);
  const tex=new THREE.CanvasTexture(c);
  const sp=new THREE.Sprite(new THREE.SpriteMaterial({map:tex, depthTest:false}));
  sp.scale.set(2.4,0.6,1); return sp;
}

function handleEvent(ev) {
  const f = ev.split(":");
  if (f[0]==="t") {
    addTracer(+f[1],+f[2],+f[3],+f[4],+f[5],+f[6]);
    const dmine=Math.hypot(+f[1]-pos.x, +f[2]-(pos.y+EYE), +f[3]-pos.z);
    if (dmine<2){ flashMuzzle(); sfxFire(0.14); } else if (dmine<28){ sfxFire(0.045*(1-dmine/28)); }
  }
  else if (f[0]==="h") { addSparks(+f[2],+f[3],+f[4], 0xff7a4a, 3); if(+f[1]===myId){ hitmark(); sfxHit(); if(f[5]) dmgNumber(+f[2],+f[3],+f[4],+f[5]); } }
  else if (f[0]==="u") { sfxUlt(); }
  else if (f[0]==="X") { bigBoom(+f[1],+f[2],+f[3]); }
  else if (f[0]==="k") { killfeed(+f[1],+f[2]); if(+f[1]===myId) sfxKill(); }
  else if (f[0]==="b") { addSparks(+f[2],1.0,+f[4], 0x6fe0ff, 14); if(+f[1]===myId) sfxBlink(); }
  else if (f[0]==="r") { addRings(+f[2],+f[4]); if(+f[1]===myId) sfxRecall(); }
  else if (f[0]==="m") { addSparks(+f[2],1.0,+f[3], 0x2ee06a, 14); if(Math.hypot(+f[2]-pos.x,+f[3]-pos.z)<3) sfxHeal(); }
}
function addTracer(x1,y1,z1,x2,y2,z2) {
  const geo=new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(x1,y1,z1),new THREE.Vector3(x2,y2,z2)]);
  const line=new THREE.Line(geo,new THREE.LineBasicMaterial({color:0xfff0a0,transparent:true,opacity:0.95}));
  scene.add(line); tracers.push({line,life:1});
}
function addSparks(x,y,z,color,n=8) {
  for(let i=0;i<n;i++){
    const m=new THREE.Mesh(new THREE.BoxGeometry(0.08,0.08,0.08), new THREE.MeshBasicMaterial({color}));
    m.position.set(x,y,z); scene.add(m);
    sparks.push({mesh:m,life:1,vel:{x:(Math.random()-.5)*6,y:Math.random()*5,z:(Math.random()-.5)*6}});
  }
}
function addRings(x,z){
  const m=new THREE.Mesh(new THREE.TorusGeometry(0.4,0.08,8,24), new THREE.MeshBasicMaterial({color:0x6fe0ff,transparent:true,opacity:0.9}));
  m.rotation.x=Math.PI/2; m.position.set(x,1,z); scene.add(m);
  sparks.push({mesh:m,life:1,vel:null,ring:true});
}
let hitTimer=0;
function hitmark(){ hitTimer=0.18; }
function killfeed(killer, victim){
  const kn = killer===myId?"You":(players.get(killer)?.name || (killer===myId?"You":"?"));
  const vn = victim===myId?"You":(players.get(victim)?.name || "?");
  feed.push({t:performance.now(), html:`<span style="color:#ffd24a">${kn}</span> ▸ ${vn}`});
  if(feed.length>6) feed.shift();
}

// ---- input ----------------------------------------------------------------
const km = {KeyW:"w",KeyA:"a",KeyS:"s",KeyD:"d",ArrowUp:"w",ArrowDown:"s",ArrowLeft:"a",ArrowRight:"d"};
addEventListener("keydown",(e)=>{
  if(!started) return;
  if(km[e.code]!==undefined) keys[km[e.code]]=true;
  if(e.code==="Space") keys.jump=true;
  if(e.code==="ShiftLeft"||e.code==="ShiftRight"){ localBlink(); ws&&ws.send("blink"); }
  if(e.code==="KeyE") ws&&ws.send("recall");
  if(e.code==="KeyQ") ws&&ws.send("ult");
  if(e.code==="KeyR") ws&&ws.send("reload");
  if(e.code==="Tab"){ $("board").style.display="flex"; renderBoard(); e.preventDefault(); }
});
addEventListener("keyup",(e)=>{
  if(km[e.code]!==undefined) keys[km[e.code]]=false;
  if(e.code==="Space") keys.jump=false;
  if(e.code==="Tab") $("board").style.display="none";
});
addEventListener("mousedown",()=>{ if(started && ws){ ws.send("fire"); firing=true; } });
addEventListener("mouseup",()=>{ if(ws){ ws.send("stop"); firing=false; } });
let firing=false;
addEventListener("mousemove",(e)=>{
  if(document.pointerLockElement){ yaw -= e.movementX*0.0022; pitch -= e.movementY*0.0022; pitch=clamp(pitch,-1.5,1.5); }
});
function localBlink(){ // predict the dash locally for instant feel
  const sy=Math.sin(yaw),cy=Math.cos(yaw); const fwd={x:-sy,z:-cy},right={x:cy,z:-sy};
  let dx=0,dz=0; if(keys.w){dx+=fwd.x;dz+=fwd.z;} if(keys.s){dx-=fwd.x;dz-=fwd.z;} if(keys.d){dx+=right.x;dz+=right.z;} if(keys.a){dx-=right.x;dz-=right.z;}
  if(Math.hypot(dx,dz)<0.01){dx=fwd.x;dz=fwd.z;} const l=Math.hypot(dx,dz); dx/=l;dz/=l;
  pos.x=clamp(pos.x+dx*BLINK_DIST,-ARENA,ARENA); pos.z=clamp(pos.z+dz*BLINK_DIST,-ARENA,ARENA);
}

// ---- prediction (mirrors server step_player) ------------------------------
function stepLocal(dt){
  const sy=Math.sin(yaw),cy=Math.cos(yaw); const fwd={x:-sy,z:-cy},right={x:cy,z:-sy};
  let wx=0,wz=0;
  if(keys.w){wx+=fwd.x;wz+=fwd.z;} if(keys.s){wx-=fwd.x;wz-=fwd.z;}
  if(keys.d){wx+=right.x;wz+=right.z;} if(keys.a){wx-=right.x;wz-=right.z;}
  const wl=Math.hypot(wx,wz); if(wl>0){wx/=wl;wz/=wl;}
  vel.x=wx*MOVE; vel.z=wz*MOVE;
  if(keys.jump&&onGround){vel.y=JUMP;onGround=false;}
  vel.y-=GRAVITY*dt;
  pos.x+=vel.x*dt; pos.y+=vel.y*dt; pos.z+=vel.z*dt;
  if(pos.y<=0){pos.y=0;vel.y=0;onGround=true;}
  pos.x=clamp(pos.x,-ARENA,ARENA); pos.z=clamp(pos.z,-ARENA,ARENA);
  for(const [cx,cz,hx,hz] of COVER){
    const nx=clamp(pos.x,cx-hx,cx+hx), nz=clamp(pos.z,cz-hz,cz+hz);
    const dx=pos.x-nx, dz=pos.z-nz, d=Math.hypot(dx,dz);
    if(d<0.45 && d>0.0001){ const push=0.45-d; pos.x+=dx/d*push; pos.z+=dz/d*push; }
  }
}

let lastSend=0;
function sendInput(now){
  if(!ws||ws.readyState!==1) return;
  if(now-lastSend < 33) return; lastSend=now;
  ws.send(`in ${+keys.w} ${+keys.s} ${+keys.a} ${+keys.d} ${+keys.jump} ${yaw.toFixed(3)} ${pitch.toFixed(3)} ${latMs.toFixed(0)}`);
}

// ---- HUD ------------------------------------------------------------------
function updateHUD(){
  $("scoreA").textContent=scoreA; $("scoreB").textContent=scoreB;
  if(!me) return;
  $("hpnum").textContent=Math.max(0,Math.round(me.hp));
  $("hpfill").style.width=clamp(me.hp/150*100,0,100)+"%";
  $("hpfill").style.background = me.hp>75?"linear-gradient(90deg,#36d27a,#9be86a)": me.hp>35?"#ffd24a":"#ff5252";
  $("ammonum").textContent=me.ammo;
  $("reload").textContent = me.reload?"RELOADING…":"";
  // blink pips
  const pips=$("blinkPips"); pips.innerHTML="";
  for(let i=0;i<3;i++){ const d=document.createElement("div"); d.className="pip"+(i<me.blink?" on":""); pips.appendChild(d); }
  $("blinkCover").style.height = me.blink<3 ? (clamp(me.blinkcd/3,0,1)*100)+"%" : "0%";
  // recall
  $("recallCover").style.height = me.recallcd>0 ? (clamp(me.recallcd/12,0,1)*100)+"%":"0%";
  $("recallCd").textContent = me.recallcd>0.1 ? Math.ceil(me.recallcd):"";
  // ult charge meter (cover recedes as it fills; "ULT" when ready)
  const u=me.ult||0; $("ultCover").style.height=(100-u)+"%";
  $("ultReady").textContent = u>=100 ? "ULT" : "";
  $("ultIcon").style.boxShadow = u>=100 ? "0 0 16px 3px #ffd24a" : "none";
  $("respawn").style.display = me.alive?"none":"flex";
}
function renderBoard(){
  const all=[...players.values()].map(e=>({name:e.name,team:e.team,elims:e.elims,deaths:e.deaths,me:false}));
  if(me) all.push({name:me.name,team:me.team,elims:me.elims,deaths:me.deaths,me:true});
  all.sort((a,b)=>b.elims-a.elims);
  $("boardBody").innerHTML = all.map(r=>`<tr class="${r.me?'me':''}"><td>${r.name}</td><td style="color:${r.team===0?'#6fb7ff':'#ff8a5a'}">${r.team===0?'BLUE':'ORANGE'}</td><td>${r.elims}</td><td>${r.deaths}</td></tr>`).join("");
}

// ---- main loop ------------------------------------------------------------
let last=performance.now();
function loop(now){
  requestAnimationFrame(loop);
  const dt=Math.min(0.05,(now-last)/1000); last=now;
  if(started){ stepLocal(dt); sendInput(now); }
  // camera (+ screen shake on explosions)
  shakeT=Math.max(0,shakeT-dt*1.6);
  const sh=shakeT*0.25;
  camera.position.set(pos.x+(Math.random()-.5)*sh, pos.y+EYE+(Math.random()-.5)*sh, pos.z+(Math.random()-.5)*sh);
  camera.rotation.y=yaw; camera.rotation.x=pitch;
  recoil*=0.8; gun.position.z=recoil*0.1; gun.rotation.x=recoil*0.3;
  if(firing && me && me.ammo>0 && !me.reload) recoil=Math.min(recoil+0.5,1.2);
  muzzleT=Math.max(0,muzzleT-dt); muzzle.material.opacity=muzzleT>0?0.9:0; muzzle.rotation.z+=0.6;
  // health packs bob/spin + availability
  for(let i=0;i<packMeshes.length;i++){ packMeshes[i].visible=packAvail[i]; packMeshes[i].rotation.y+=dt*1.5; packMeshes[i].position.y=1.0+Math.sin(now/400+i)*0.15; }
  // damage / reload feedback
  if(me){
    if(me.hp < prevHp-0.5){ if(!shot) flashDmg(); sfxHurt(); }
    prevHp=me.hp;
    if(me.reload && !prevReload) sfxReload();
    prevReload=me.reload;
  }
  // remote interpolation
  for(const e of players.values()){
    e.grp.position.x+=(e.tx-e.grp.position.x)*Math.min(1,dt*14);
    e.grp.position.y+=(e.ty-e.grp.position.y)*Math.min(1,dt*14);
    e.grp.position.z+=(e.tz-e.grp.position.z)*Math.min(1,dt*14);
    e.grp.rotation.y = e.tyaw+Math.PI;
  }
  // tracers
  for(let i=tracers.length-1;i>=0;i--){ const t=tracers[i]; t.life-=dt*12; t.line.material.opacity=Math.max(0,t.life);
    if(t.life<=0){ scene.remove(t.line); tracers.splice(i,1); } }
  // sparks
  for(let i=sparks.length-1;i>=0;i--){ const s=sparks[i]; s.life-=dt*2.5;
    if(s.ring){ s.mesh.scale.setScalar(1+(1-s.life)*5); s.mesh.material.opacity=Math.max(0,s.life); }
    else { s.vel.y-=14*dt; s.mesh.position.x+=s.vel.x*dt; s.mesh.position.y+=s.vel.y*dt; s.mesh.position.z+=s.vel.z*dt; s.mesh.scale.setScalar(Math.max(0.01,s.life)); }
    if(s.life<=0){ scene.remove(s.mesh); sparks.splice(i,1); } }
  // hitmarker
  hitTimer=Math.max(0,hitTimer-dt); $("hitm").style.opacity=hitTimer>0?1:0;
  // killfeed
  const fnow=performance.now(); feed=feed.filter(f=>fnow-f.t<4500);
  $("feed").innerHTML=feed.map(f=>`<div>${f.html}</div>`).join("");
  renderer.render(scene,camera);
  window.__alive = started && !!(me && me.alive);
  if(shot) driveShot(now);
}
requestAnimationFrame(loop);

// crosshair
{ const c=$("cross"); const mk=(w,h,x,y)=>{const d=document.createElement("div");d.style.width=w+"px";d.style.height=h+"px";d.style.left=x+"px";d.style.top=y+"px";c.appendChild(d);};
  mk(2,10,-1,-13); mk(2,10,-1,3); mk(10,2,-13,-1); mk(10,2,3,-1); }

// ---- start / pointer lock -------------------------------------------------
function start(){
  started=true; $("overlay").style.display="none";
  const c=ac(); if(c && c.state==="suspended") c.resume();
  connect(($("nick").value.trim())||("Tracer"+(Math.random()*900+100|0)));
  if(!shot) renderer.domElement.requestPointerLock();
}
$("start").onclick=start;
$("overlay").onclick=(e)=>{ if(e.target===$("overlay")) start(); };
renderer.domElement.addEventListener("click",()=>{ if(started && !shot && !document.pointerLockElement) renderer.domElement.requestPointerLock(); });

// ---- headless screenshot driver ------------------------------------------
let shotStart=0;
function driveShot(now){
  if(!shotStart) shotStart=now;
  const t=(now-shotStart)/1000;
  yaw=0; pitch=-0.04;                   // look down-field toward the enemy team
  keys.a = Math.floor(t*1.5)%2===0; keys.d=!keys.a;  // strafe to dodge bot fire
  if(t>0.5 && !firing && ws && ws.readyState===1){ ws.send("fire"); firing=true; }
}
if(shot){ // auto-join for the capture
  $("nick").value="Tracer"; start();
}
