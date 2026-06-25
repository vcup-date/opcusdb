// opcusdb Warfront — RTS client. Server streams only units inside your camera
// (spatial AOI). Rendered as a battlefield: terrain, keeps, soldier chevrons that
// face their march, gunfire tracers and death bursts.

const $ = (id) => document.getElementById(id);
const cv = $("cv"), ctx = cv.getContext("2d");
const W = 4000, H = 2600;
const BASEX = [200, W - 200], BASEY = H / 2;
const COL = ["#5b9dff", "#ff5d5d"], COLD = ["#1c3a6b", "#6b1f24"];

let ws = null, myId = 0, started = false;
let cam = { x: W/2, y: H/2, scale: 0.5 };
let units = new Map();      // id -> {x,y,team,hp,fa}
const face = new Map();     // id -> angle (persisted)
let selected = new Set();
let deaths = [], tracers = [];
let baseHp = [4000, 4000], baseMax = [4000, 4000];
let cntA = 0, cntB = 0, time = 0;
const keys = {};
let box = null, moveMark = null;

function resize(){ cv.width = innerWidth; cv.height = innerHeight; }
addEventListener("resize", resize); resize();
const toS = (x, y) => [(x - cam.x) * cam.scale + cv.width/2, (y - cam.y) * cam.scale + cv.height/2];
const toW = (sx, sy) => [(sx - cv.width/2) / cam.scale + cam.x, (sy - cv.height/2) / cam.scale + cam.y];

// terrain decorations (static, world coords)
const patches = [];
(function(){ let s = 12345; const rnd = () => (s = (s*1103515245+12345) & 0x7fffffff) / 0x7fffffff;
  for (let i=0;i<70;i++) patches.push({ x: rnd()*W, y: rnd()*H, r: 60+rnd()*180, c: rnd()<0.5?"#223a1e":"#2e4a26" }); })();

// ---- networking -----------------------------------------------------------
function connect(){
  ws = new WebSocket(`ws://${location.host}/ws`);
  ws.onopen = () => sendView(true);
  ws.onmessage = (e) => {
    for (const line of e.data.split("\n")) {
      if (!line) continue;
      const i = line.indexOf("\t"), tag = i<0?line:line.slice(0,i), rest = i<0?"":line.slice(i+1);
      if (tag === "w") myId = +rest.split("\t")[0];
      else if (tag === "g") { const p = rest.split("\t"); cntA=+p[0]; cntB=+p[1]; time=+p[2]; }
      else if (tag === "b") { const p = rest.split("\t"); baseHp=[+p[0],+p[2]]; baseMax=[+p[1],+p[3]]; }
      else if (tag === "u") {
        const next = new Map();
        for (const s of rest.split(";")) { if (!s) continue; const a = s.split(","); const id=+a[0], x=+a[1], y=+a[2];
          const pv = units.get(id); let fa = face.get(id) || (a[3]==="0"?0:Math.PI);
          if (pv){ const dx=x-pv.x, dy=y-pv.y; if (dx*dx+dy*dy>0.4){ fa=Math.atan2(dy,dx); face.set(id,fa);} }
          next.set(id, { x, y, team:+a[3], hp:+a[4], fa }); }
        units = next;
      } else if (tag === "f") {
        for (const s of rest.split(";")) { if (!s) continue; const a=s.split(","); tracers.push({x1:+a[0],y1:+a[1],x2:+a[2],y2:+a[3],team:+a[4],life:1}); }
      } else if (tag === "x") {
        for (const s of rest.split(";")) { if (!s) continue; const a=s.split(","); deaths.push({x:+a[0],y:+a[1],team:+a[2],life:1}); }
      }
    }
  };
}
let lastView = 0;
function sendView(force){ if (!ws||ws.readyState!==1) return; const now=performance.now(); if(!force&&now-lastView<90)return; lastView=now;
  ws.send(`view ${cam.x.toFixed(0)} ${cam.y.toFixed(0)} ${(cv.width/2/cam.scale).toFixed(0)} ${(cv.height/2/cam.scale).toFixed(0)}`); }

// ---- input ----------------------------------------------------------------
addEventListener("keydown", e => keys[e.key.toLowerCase()] = true);
addEventListener("keyup", e => keys[e.key.toLowerCase()] = false);
cv.addEventListener("wheel", (e)=>{ e.preventDefault(); cam.scale=Math.min(1.8,Math.max(0.18,cam.scale*Math.exp(-e.deltaY*0.0011))); sendView(true); }, {passive:false});
cv.addEventListener("mousedown", (e)=>{ if(started && e.button===0) box={x0:e.clientX,y0:e.clientY,x1:e.clientX,y1:e.clientY}; });
cv.addEventListener("mousemove", (e)=>{ if(box){box.x1=e.clientX;box.y1=e.clientY;} });
addEventListener("mouseup", ()=>{ if(!box) return;
  const moved=Math.hypot(box.x1-box.x0,box.y1-box.y0)>5; selected.clear();
  if(moved){ const [a0,b0]=toW(Math.min(box.x0,box.x1),Math.min(box.y0,box.y1)); const [a1,b1]=toW(Math.max(box.x0,box.x1),Math.max(box.y0,box.y1));
    for(const [id,u] of units) if(u.team===0&&u.x>=a0&&u.x<=a1&&u.y>=b0&&u.y<=b1) selected.add(id);
  } else { const [wx,wy]=toW(box.x0,box.y0); let best=null,bd=1e9; for(const [id,u] of units) if(u.team===0){const d=(u.x-wx)**2+(u.y-wy)**2; if(d<bd&&d<(28/cam.scale)**2){bd=d;best=id;}} if(best!=null)selected.add(best); }
  box=null; });
cv.addEventListener("contextmenu", (e)=>{ e.preventDefault(); if(!started||!selected.size)return; const [wx,wy]=toW(e.clientX,e.clientY);
  moveMark={x:wx,y:wy,life:1}; ws&&ws.readyState===1&&ws.send(`order ${wx.toFixed(0)} ${wy.toFixed(0)} ${[...selected].slice(0,1800).join(",")}`); });

// ---- render ---------------------------------------------------------------
let last = performance.now();
function loop(now){
  requestAnimationFrame(loop);
  const dt = Math.min(0.05,(now-last)/1000); last=now;
  const pan = 700*dt/cam.scale;
  if(keys.a||keys.arrowleft)cam.x-=pan; if(keys.d||keys.arrowright)cam.x+=pan; if(keys.w||keys.arrowup)cam.y-=pan; if(keys.s||keys.arrowdown)cam.y+=pan;
  cam.x=Math.max(0,Math.min(W,cam.x)); cam.y=Math.max(0,Math.min(H,cam.y));
  if(keys.a||keys.d||keys.w||keys.s||keys.arrowleft||keys.arrowright||keys.arrowup||keys.arrowdown) sendView();

  // terrain
  ctx.fillStyle="#0a0d12"; ctx.fillRect(0,0,cv.width,cv.height);
  const [bx0,by0]=toS(0,0);
  ctx.fillStyle="#1d3a1d"; ctx.fillRect(bx0,by0,W*cam.scale,H*cam.scale);
  ctx.save(); ctx.beginPath(); ctx.rect(bx0,by0,W*cam.scale,H*cam.scale); ctx.clip();
  for(const p of patches){ const [px,py]=toS(p.x,p.y); ctx.fillStyle=p.c; ctx.beginPath(); ctx.ellipse(px,py,p.r*cam.scale,p.r*0.7*cam.scale,0,0,7); ctx.fill(); }
  // no-man's-land stripe
  const [mx]=toS(W/2,0); ctx.fillStyle="#3a3320"; ctx.globalAlpha=0.25; ctx.fillRect(mx-30*cam.scale,by0,60*cam.scale,H*cam.scale); ctx.globalAlpha=1;
  ctx.restore();
  ctx.strokeStyle="#0c1208"; ctx.lineWidth=3; ctx.strokeRect(bx0,by0,W*cam.scale,H*cam.scale);

  drawBase(0); drawBase(1);
  drawTeam(0); drawTeam(1);

  // selection rings
  ctx.strokeStyle="#eafff6"; ctx.lineWidth=1.5;
  for(const id of selected){ const u=units.get(id); if(!u)continue; const [sx,sy]=toS(u.x,u.y); ctx.beginPath(); ctx.arc(sx,sy,Math.max(5,8*cam.scale),0,7); ctx.stroke(); }

  // tracers
  for(const t of tracers){ const [a,b]=toS(t.x1,t.y1),[c,d]=toS(t.x2,t.y2); ctx.strokeStyle=`rgba(255,225,140,${t.life*0.8})`; ctx.lineWidth=1.4;
    ctx.beginPath(); ctx.moveTo(a,b); ctx.lineTo(c,d); ctx.stroke(); t.life-=dt*6; }
  tracers=tracers.filter(t=>t.life>0);
  // death bursts
  for(const dh of deaths){ const [sx,sy]=toS(dh.x,dh.y); const r=(1-dh.life)*10+2; ctx.fillStyle=`rgba(255,150,60,${dh.life})`; ctx.beginPath(); ctx.arc(sx,sy,r*cam.scale+1.5,0,7); ctx.fill(); dh.life-=dt*2.5; }
  deaths=deaths.filter(d=>d.life>0);
  // move marker
  if(moveMark){ const [sx,sy]=toS(moveMark.x,moveMark.y); ctx.strokeStyle=`rgba(110,230,160,${moveMark.life})`; ctx.lineWidth=2; ctx.beginPath(); ctx.arc(sx,sy,18*(1.3-moveMark.life)+4,0,7); ctx.stroke(); moveMark.life-=dt*1.6; if(moveMark.life<=0)moveMark=null; }
  // selection box
  if(box){ const x=Math.min(box.x0,box.x1),y=Math.min(box.y0,box.y1),w=Math.abs(box.x1-box.x0),h=Math.abs(box.y1-box.y0);
    ctx.strokeStyle="#7dffbf"; ctx.fillStyle="rgba(125,255,191,0.08)"; ctx.lineWidth=1.5; ctx.fillRect(x,y,w,h); ctx.strokeRect(x,y,w,h); }

  $("cA").textContent=cntA; $("cB").textContent=cntB; $("sel").textContent=selected.size+" selected"; $("time").textContent=Math.floor(time)+"s";
  const over = started && (baseHp[0]<=0 || baseHp[1]<=0);
  $("over").style.display = over?"flex":"none";
  if(over){ $("overtxt").textContent = baseHp[1]<=0?"VICTORY":"DEFEAT"; $("overtxt").style.color = baseHp[1]<=0?"#6fb0ff":"#ff7a7a"; }
}
function drawTeam(team){
  const r = Math.max(3.5, Math.min(16, 13*cam.scale));
  for(const u of units.values()){ if(u.team!==team) continue; const [sx,sy]=toS(u.x,u.y);
    if(sx<-8||sy<-8||sx>cv.width+8||sy>cv.height+8) continue;
    ctx.save(); ctx.translate(sx,sy); ctx.rotate(u.fa);
    ctx.globalAlpha=0.55+0.45*(u.hp/9);
    ctx.beginPath(); ctx.moveTo(r,0); ctx.lineTo(-r*0.7,r*0.6); ctx.lineTo(-r*0.35,0); ctx.lineTo(-r*0.7,-r*0.6); ctx.closePath();
    ctx.fillStyle=COL[team]; ctx.fill(); ctx.lineWidth=0.8; ctx.strokeStyle=COLD[team]; ctx.stroke();
    ctx.restore(); }
  ctx.globalAlpha=1;
}
function drawBase(team){
  const [sx,sy]=toS(BASEX[team],BASEY); const w=120*cam.scale, h=120*cam.scale;
  ctx.fillStyle="#3a3f4a"; ctx.fillRect(sx-w/2,sy-h/2,w,h);
  ctx.fillStyle=COLD[team]; ctx.fillRect(sx-w/2,sy-h/2,w,h*0.18);
  // crenellations
  ctx.fillStyle="#4a505c"; for(let i=0;i<5;i++) ctx.fillRect(sx-w/2+i*w/5, sy-h/2-h*0.1, w/9, h*0.1);
  // banner
  ctx.fillStyle=COL[team]; ctx.fillRect(sx-3, sy-h/2-h*0.45, 6, h*0.35); ctx.fillRect(sx-3, sy-h/2-h*0.45, w*0.22, h*0.18);
  // hp bar
  const f = Math.max(0, baseHp[team]/baseMax[team]); const bw=w*1.1;
  ctx.fillStyle="#000a"; ctx.fillRect(sx-bw/2, sy-h/2-h*0.62, bw, 7);
  ctx.fillStyle=COL[team]; ctx.fillRect(sx-bw/2, sy-h/2-h*0.62, bw*f, 7);
}
requestAnimationFrame(loop);

function start(){ started=true; $("join").style.display="none"; resize(); connect(); }
$("go").onclick = start;
