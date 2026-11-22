// Canvas rendering is intentional: it works offline, in headless browsers, and without WebGL.
export async function createHelix(container) {
  const canvas = document.createElement('canvas');
  canvas.setAttribute('aria-hidden', 'true');
  container.replaceChildren(canvas);
  const ctx = canvas.getContext('2d');
  if (!ctx) {
    const fallback = document.createElement('div');
    fallback.className = 'helix-fallback';
    fallback.setAttribute('aria-hidden', 'true');
    container.replaceChildren(fallback);
    return {update() {}, setHits() {}, renderer: 'CSS fallback'};
  }
  let width = 1, height = 1, fraction = 0, hits = [], stopped = false, frame = 0;
  let phase = .7;
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  const defaults = ['TCF7L2', 'KCNJ11', 'PPARG', 'FTO', 'SLC30A8', 'APOE', 'HFE', 'F5', 'F2', 'LCT'];
  const colors = ['#01eff1', '#f17fbd', '#74d3c2', '#a9a0f7'];
  const project = (x, y, z, time) => {
    const turn = motion.matches ? 0 : time * .000075;
    // Several turns along the molecule make the 2D/WebGL fallback a helix,
    // rather than the two straight rails produced by the old nearly-flat angle.
    const a = phase + y * Math.PI * 5.2 + turn;
    const depth = Math.cos(a);
    const spread = Math.min(width * .225, height * .245, 160);
    const perspective = .76 + (depth + 1) * .12;
    return {x: width * .5 + Math.sin(a) * spread * perspective, y: height * .10 + y * height * .80, z: depth};
  };
  function draw(time = 0) {
    if (stopped) return;
    ctx.clearRect(0, 0, width, height);
    const halo = ctx.createRadialGradient(width*.5,height*.5,10,width*.5,height*.5,Math.min(width,height)*.49);
    halo.addColorStop(0,'rgba(0,117,142,.17)'); halo.addColorStop(.65,'rgba(0,68,89,.04)'); halo.addColorStop(1,'transparent');
    ctx.fillStyle=halo;ctx.fillRect(0,0,width,height);
    // Quiet orbital geometry makes the canvas feel like an instrument, not an illustration.
    ctx.save();ctx.translate(width*.5,height*.5);ctx.strokeStyle='rgba(54,162,184,.085)';ctx.lineWidth=1;
    for (const [rx,ry,rot] of [[.40,.37,-.32],[.33,.44,.4],[.44,.28,.13]]) {
      ctx.beginPath();ctx.ellipse(0,0,width*rx,height*ry,rot,0,Math.PI*2);ctx.stroke();
    }
    ctx.setLineDash([3,9]);ctx.beginPath();ctx.moveTo(0,-height*.45);ctx.lineTo(0,height*.45);ctx.stroke();ctx.restore();
    const count = 68;
    const first=[],second=[];
    for(let i=0;i<count;i++) {
      const y=i/(count-1);
      const a=project(0,y,0,time),b={...a,x:width-a.x,z:-a.z};
      first.push(a);second.push(b);
    }
    // Back-to-front painting gives rungs perspective and keeps the
    // rear strand visibly dimmer wherever the two sine strands cross.
    const pieces=[];
    for(let i=2;i<count-2;i+=3) {
      const a=first[i],b=second[i],depth=(a.z+b.z)/2;
      const trim=2+(depth+1)*1.5;
      const dx=b.x-a.x,dy=b.y-a.y,length=Math.hypot(dx,dy)||1;
      const ax=a.x+dx/length*trim,ay=a.y+dy/length*trim;
      const bx=b.x-dx/length*trim,by=b.y-dy/length*trim;
      const gradient=ctx.createLinearGradient(ax,ay,bx,by);
      gradient.addColorStop(0,'rgba(0,222,239,.64)');gradient.addColorStop(.48,'rgba(106,171,194,.19)');gradient.addColorStop(1,'rgba(232,102,181,.56)');
      pieces.push({z:Math.min(a.z,b.z)-.15,draw(){
        ctx.globalAlpha=.22+(depth+1)*.25;ctx.strokeStyle=gradient;ctx.lineWidth=.7+(depth+1)*1.1;
        ctx.beginPath();ctx.moveTo(ax,ay);ctx.lineTo(bx,by);ctx.stroke();ctx.globalAlpha=1;
      }});
    }
    for(const [points,color] of [[first,'#01e3fa'],[second,'#ed79bd']]) {
      for(let i=1;i<points.length;i++) {
        const a=points[i-1],b=points[i],depth=(a.z+b.z)/2;
        pieces.push({z:depth,draw(){
          ctx.strokeStyle=color;ctx.globalAlpha=.23+(depth+1)*.34;ctx.lineWidth=1.1+(depth+1)*.8;
          ctx.beginPath();ctx.moveTo(a.x,a.y);ctx.lineTo(b.x,b.y);ctx.stroke();ctx.globalAlpha=1;
        }});
      }
      for(const p of points) {
        pieces.push({z:p.z+.02,draw(){
          const radius=1.8+(p.z+1)*2;
          ctx.shadowColor=color;ctx.shadowBlur=3+(p.z+1)*3;ctx.globalAlpha=.35+(p.z+1)*.325;
          const g=ctx.createRadialGradient(p.x-radius*.4,p.y-radius*.4,0,p.x,p.y,radius);
          g.addColorStop(0,'#eaffff');g.addColorStop(.25,color);g.addColorStop(.75,color);g.addColorStop(1,'rgba(0,35,48,.85)');
          ctx.fillStyle=g;ctx.beginPath();ctx.arc(p.x,p.y,radius,0,Math.PI*2);ctx.fill();
          ctx.shadowBlur=0;ctx.globalAlpha=1;
        }});
      }
    }
    pieces.sort((a,b)=>a.z-b.z).forEach(piece=>piece.draw());
    // The scan ring follows input bytes, not wall-clock animation.
    const scanY=height*(.10+fraction*.80);
    ctx.save();ctx.shadowColor='#17fcfd';ctx.shadowBlur=20;
    const ring=ctx.createLinearGradient(width*.25,0,width*.75,0);ring.addColorStop(0,'transparent');ring.addColorStop(.3,'#17fcfd');ring.addColorStop(.7,'#17fcfd');ring.addColorStop(1,'transparent');
    ctx.strokeStyle=ring;ctx.lineWidth=1.6;ctx.beginPath();ctx.ellipse(width*.5,scanY,Math.min(width*.29,180),13,0,0,Math.PI*2);ctx.stroke();ctx.restore();
    const genes = hits.length ? hits.slice(0,10).map(h=>({...h})) : defaults.map(gene=>({gene,alt_votes:0,idle:true}));
    ctx.font='600 10px Inter, system-ui, sans-serif';ctx.textBaseline='middle';
    const narrow=width<380;
    const spread=Math.min(width*.225,height*.245,160);
    genes.forEach((hit,i)=>{
      const y=(i+.55)/genes.length;
      const side=i%2===0?1:-1;
      const p=project(0,y,0,0);p.x=width*.5+side*spread*.83;
      const dotY=height*(.10+y*.80);
      const w=ctx.measureText(hit.gene).width+18;
      const x=side>0?Math.min(width-w-8,width*.65):Math.max(8,width*.35-w);
      const lineEnd=side>0?x:x+w;
      const c=hit.idle?'#3d6873':hit.alt_votes>0?'#fbac1b':'#01eff1';
      ctx.strokeStyle=c;ctx.globalAlpha=hit.idle?.38:.72;ctx.lineWidth=.8;ctx.beginPath();ctx.moveTo(p.x,dotY);ctx.lineTo((p.x+lineEnd)/2,dotY);ctx.lineTo(lineEnd,dotY+(y*height<scanY?-1:1)*11);ctx.stroke();
      ctx.fillStyle=c;ctx.globalAlpha=1;ctx.beginPath();ctx.arc(p.x,dotY,2.2,0,Math.PI*2);ctx.fill();
      ctx.fillStyle=hit.idle?'rgba(2,19,27,.60)':'rgba(1,16,23,.90)';ctx.strokeStyle=c;ctx.lineWidth=.8;
      ctx.beginPath();ctx.roundRect(x,dotY+(y*height<scanY?-24:-2),w,22,5);ctx.fill();ctx.globalAlpha=hit.idle?.40:.85;ctx.stroke();ctx.globalAlpha=1;
      ctx.fillStyle=hit.idle?'#42656e':c;ctx.fillText(hit.gene,x+9,dotY+(y*height<scanY?-13:9));
    });
    ctx.font='8px ui-monospace, monospace';ctx.fillStyle='rgba(125,180,193,.5)';
    ctx.fillText('5′',width*.5-spread-20,height*.075);ctx.fillText('3′',width*.5+spread+10,height*.075);
    ctx.fillText('3′',width*.5-spread-20,height*.94);ctx.fillText('5′',width*.5+spread+10,height*.94);
    if(!motion.matches) frame=requestAnimationFrame(draw);
  }
  const resize=()=>{
    width=Math.max(container.clientWidth,1);height=Math.max(container.clientHeight,1);
    const dpr=Math.min(window.devicePixelRatio||1,2);
    canvas.width=Math.round(width*dpr);canvas.height=Math.round(height*dpr);
    canvas.style.width=`${width}px`;canvas.style.height=`${height}px`;ctx.setTransform(dpr,0,0,dpr,0,0);draw(performance.now());
  };
  const observer = new ResizeObserver(resize);observer.observe(container);resize();
  const dispose=()=>{stopped=true;cancelAnimationFrame(frame);observer.disconnect();};
  window.disposeHelix=dispose;
  return {
    renderer: '2D canvas',
    update(value) {fraction=Math.min(Math.max(Number(value)||0,0),1);if(motion.matches)draw(0);},
    setHits(value) {hits=Array.isArray(value)?value:[];if(motion.matches)draw(0);},
    setHit() {},
  };
}
export function disposeHelix() {if(window.disposeHelix)window.disposeHelix();}
