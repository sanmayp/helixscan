// The supplied artwork is primary; the canvas above it contains only live evidence.
// A drawn helix is used solely when embedded video cannot be played.
export async function createHelix(container, options = {}) {
  const canvas = document.createElement('canvas');
  canvas.className = 'helix-overlay';
  canvas.setAttribute('aria-hidden', 'true');
  const media = document.createElement('div');
  media.className = 'helix-media';
  media.setAttribute('aria-hidden', 'true');
  const poster = document.createElement('img');
  poster.className = 'helix-art-poster';
  poster.src = '/art/00-helix.webp';
  poster.alt = '';
  poster.decoding = 'async';
  poster.addEventListener('error', () => { poster.hidden = true; useFallback(); });
  media.append(poster);
  const fallback = document.createElement('div');
  fallback.className = 'helix-fallback';
  fallback.hidden = true;
  media.append(fallback);
  container.replaceChildren(media, canvas);
  const ctx = canvas.getContext('2d');
  const fallbackCanvas = document.createElement('canvas');
  fallbackCanvas.className = 'helix-fallback-canvas';
  fallbackCanvas.setAttribute('aria-hidden', 'true');
  fallback.append(fallbackCanvas);
  const fallbackCtx = fallbackCanvas.getContext('2d');
  let width = 1, height = 1, fraction = 0, hits = [], stopped = false;
  const panelGenes = Array.isArray(options.genes) ? options.genes : [];
  const defaults = ['TCF7L2', 'KCNJ11', 'PPARG', 'FTO', 'SLC30A8', 'APOE', 'HFE', 'F5', 'F2', 'LCT'];
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  const videos = [];
  let active = 0, fadeFrame = 0, ready = false;

  function useFallback() {
    if (stopped || fallback.hidden === false) return;
    fallback.hidden = false;
    poster.hidden = true;
    container.classList.add('helix-art-fallback');
    for (const video of videos) video.pause();
    cancelAnimationFrame(fadeFrame);
  }
  function resize() {
    width = Math.max(container.clientWidth, 1);
    height = Math.max(container.clientHeight, 1);
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    fallbackCanvas.width = Math.round(width * dpr);
    fallbackCanvas.height = Math.round(height * dpr);
    fallbackCtx?.setTransform(dpr, 0, 0, dpr, 0, 0);
    drawFallback();
    ctx?.setTransform(dpr, 0, 0, dpr, 0, 0);
    drawOverlay();
  }
  function drawOverlay() {
    if (!ctx || stopped) return;
    ctx.clearRect(0, 0, width, height);
    // This is a scan-position indicator, not wall-clock animation.
    const scanY = height * (.10 + fraction * .80);
    ctx.save();
    ctx.shadowColor = '#17fcfd';
    ctx.shadowBlur = 16;
    const ring = ctx.createLinearGradient(width * .22, 0, width * .78, 0);
    ring.addColorStop(0, 'transparent');
    ring.addColorStop(.3, '#17fcfd');
    ring.addColorStop(.7, '#17fcfd');
    ring.addColorStop(1, 'transparent');
    ctx.strokeStyle = ring;
    ctx.lineWidth = 1.4;
    ctx.beginPath();
    ctx.ellipse(width * .5, scanY, Math.min(width * .26, 150), 10, 0, 0, Math.PI * 2);
    ctx.stroke();
    ctx.restore();
    // Keep the panel visible before evidence arrives; hit genes light up in place.
    const genes = (panelGenes.length ? panelGenes : defaults).map((gene, i) => {
      const hit = hits.find(h => h.gene === gene || gene.split('/').includes(h.gene));
      return {gene: hit?.gene || gene, alt_votes: hit?.alt_votes || 0, idle: !hit, i};
    }).sort((a, b) => Number(a.idle) - Number(b.idle) || a.i - b.i).slice(0, 15);
    if (!genes.length) return;
    ctx.font = '600 10px Inter, system-ui, sans-serif';
    ctx.textBaseline = 'middle';
    const spread = Math.min(width * .225, height * .245, 160);
    genes.forEach((hit, i) => {
      const y = (i + .55) / genes.length;
      const side = i % 2 === 0 ? 1 : -1;
      const x0 = width * .5 + side * spread * .83;
      const y0 = height * (.10 + y * .80);
      const labelWidth = ctx.measureText(hit.gene).width + 18;
      const x = side > 0 ? Math.min(width - labelWidth - 8, width * .65) : Math.max(8, width * .35 - labelWidth);
      const lineEnd = side > 0 ? x : x + labelWidth;
      const color = hit.idle ? '#42656e' : hit.alt_votes > 0 ? '#faac41' : '#01eff1';
      ctx.strokeStyle = color;
      ctx.globalAlpha = hit.idle ? .42 : .85;
      ctx.lineWidth = .8;
      ctx.beginPath();
      ctx.moveTo(x0, y0);
      ctx.lineTo((x0 + lineEnd) / 2, y0);
      ctx.lineTo(lineEnd, y0 + (y0 < scanY ? -1 : 1) * 11);
      ctx.stroke();
      ctx.fillStyle = color;
      ctx.beginPath();
      ctx.arc(x0, y0, 2.2, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = hit.idle ? 'rgba(2,19,27,.65)' : 'rgba(1,16,23,.90)';
      ctx.beginPath();
      ctx.roundRect(x, y0 + (y0 < scanY ? -24 : -2), labelWidth, 22, 5);
      ctx.fill();
      ctx.globalAlpha = hit.idle ? .42 : .85;
      ctx.stroke();
      ctx.globalAlpha = 1;
      ctx.fillStyle = hit.idle ? '#5b7c85' : color;
      ctx.fillText(hit.gene, x + 9, y0 + (y0 < scanY ? -13 : 9));
    });
  }
  function drawFallback() {
    if (!fallbackCtx) return;
    const c=fallbackCtx;
    c.clearRect(0,0,width,height);
    const top=height*.04,bottom=height*.96;
    const points=110;
    for(let strand=0;strand<2;strand++) {
      const color=strand===0?'#01e3fa':'#ed79bd';
      c.strokeStyle=color;c.lineWidth=2.5;c.shadowColor=color;c.shadowBlur=10;
      c.beginPath();
      for(let i=0;i<=points;i++) {
        const y=top+(bottom-top)*i/points;
        const x=width*.5+Math.sin(i/points*Math.PI*5+strand*Math.PI)*width*.14;
        i?c.lineTo(x,y):c.moveTo(x,y);
      }
      c.stroke();
    }
    c.shadowBlur=0;
    for(let i=0;i<20;i++) {
      const y=top+(bottom-top)*(i+.5)/20;
      const phase=(i+.5)/20*Math.PI*5;
      const x1=width*.5+Math.sin(phase)*width*.14;
      const x2=width*.5-Math.sin(phase)*width*.14;
      c.strokeStyle=i%2?'#17fcfd':'#bb7dcc';c.lineWidth=.8;
      c.beginPath();c.moveTo(x1,y);c.lineTo(x2,y);c.stroke();
      c.fillStyle=i%2?'#01eff1':'#ed79bd';
      c.beginPath();c.arc(x1,y,2,0,Math.PI*2);c.fill();
      c.beginPath();c.arc(x2,y,2,0,Math.PI*2);c.fill();
    }
  }
  function makeVideo(index) {
    const video = document.createElement('video');
    video.className = 'helix-art-video';
    video.dataset.layer = String(index);
    video.muted = true;
    video.defaultMuted = true;
    video.playsInline = true;
    video.preload = 'auto';
    video.loop = false;
    video.setAttribute('aria-hidden', 'true');
    video.src = '/art/09-helix-loop.mp4';
    video.addEventListener('error', useFallback);
    media.append(video);
    videos.push(video);
    return video;
  }
  async function crossfade(duration) {
    const outgoing = videos[active], incoming = videos[1 - active];
    if (stopped || !Number.isFinite(duration) || duration < 1.6) { useFallback(); return; }
    const fadeSeconds = Math.min(.6, duration / 4);
    const startAt = Math.max(0, duration - fadeSeconds);
    let fading = false;
    const tick = async () => {
      if (stopped || !ready) return;
      if (!fading && outgoing.currentTime >= startAt) {
        fading = true;
        try { incoming.currentTime = 0; await incoming.play(); }
        catch { useFallback(); return; }
      }
      if (fading) {
        // If playback pauses at the last frame, use elapsed time as a second clock.
        if (!fadeStartedAt) fadeStartedAt = performance.now();
        const opacity = Math.min(1, Math.max(
          (outgoing.currentTime - startAt) / fadeSeconds,
          (performance.now() - fadeStartedAt) / (fadeSeconds * 1000),
        ));
        outgoing.style.opacity = String((1 - opacity) * .70);
        incoming.style.opacity = String(opacity * .70);
        if (opacity >= 1) {
          outgoing.pause();
          outgoing.style.opacity = '';
          outgoing.classList.remove('is-active');
          incoming.style.opacity = '';
          incoming.classList.add('is-active');
          active = 1 - active;
          fadeStartedAt = 0;
          fadeFrame = requestAnimationFrame(() => crossfade(videos[active].duration));
          return;
        }
      } else { fadeStartedAt = 0; }
      fadeFrame = requestAnimationFrame(tick);
    };
    let fadeStartedAt = 0;
    fadeFrame = requestAnimationFrame(tick);
  }
  async function startVideo() {
    if (motion.matches || !ctx) {
      if (motion.matches) motionFreeze();
      if (!ctx) useFallback();
      return { renderer: motion.matches ? 'embedded WebP poster + 2D overlay' : 'CSS fallback' };
    }
    const first = makeVideo(0), second = makeVideo(1);
    const canPlay = new Promise(resolve => {
      if (first.readyState >= 3) { resolve(true); return; }
      first.addEventListener('canplay', () => resolve(true), { once: true });
      first.addEventListener('error', () => resolve(false), { once: true });
    });
    const loaded = await Promise.race([
      canPlay,
      new Promise(resolve => setTimeout(() => resolve(first.readyState >= 3), 8000)),
    ]);
    if (stopped) return { renderer: 'disposed' };
    if (!loaded || !Number.isFinite(first.duration) || first.duration < 1.6) {
      useFallback();
      return { renderer: 'CSS fallback' };
    }
    try {
      await first.play();
    } catch {
      useFallback();
      return { renderer: 'CSS fallback' };
    }
    ready = true;
    first.classList.add('is-active');
    container.classList.add('helix-art-ready');
    poster.style.animation = 'none';
    second.load();
    // Two independently buffered videos overlap across the source clip seam.
    crossfade(first.duration);
    return { renderer: 'embedded MP4 + transparent live-evidence canvas' };
  }
  const observer = new ResizeObserver(resize);
  observer.observe(container);
  resize();
  const motionFreeze = () => { for (const video of videos) video.pause(); cancelAnimationFrame(fadeFrame); };
  const renderer = await startVideo();
  motion.addEventListener('change', () => { if (motion.matches) motionFreeze(); });
  const dispose = () => {
    stopped = true;
    cancelAnimationFrame(fadeFrame);
    observer.disconnect();
    for (const video of videos) video.pause();
  };
  window.disposeHelix = dispose;
  return {
    renderer,
    update(value) { fraction = Math.min(Math.max(Number(value) || 0, 0), 1); drawOverlay(); },
    setHits(value) { hits = Array.isArray(value) ? value : []; drawOverlay(); },
    setPanelGenes(value) { if(Array.isArray(value)) { panelGenes.splice(0,panelGenes.length,...value); drawOverlay(); } },
    setHit() {},
  };
}
export function disposeHelix() { if (window.disposeHelix) window.disposeHelix(); }
