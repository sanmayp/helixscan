import { createHelix, disposeHelix } from './helix.js';
// Report rendering is integrated after the scan controller below.

const $ = id => document.getElementById(id);
const safe = value => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const number = new Intl.NumberFormat(undefined, {maximumFractionDigits: 0});
const compact = new Intl.NumberFormat(undefined, {notation: 'compact', maximumFractionDigits: 1});
const RECENT_KEY = 'helixscan.recent.v1';
export const DISCLAIMER = 'Research and education only. Not a medical diagnosis. Talk to a doctor or genetic counselor about any result.';
let selectedName = '', selectedSize = 0, activeId = '', lastResult = null, reportUrl = '';
let eventSource = null, pollTimer = null, telemetryTimer = null, completedOnScan = false;
let helix = null, logicalCores = 0, recentReports = readRecent();

function readRecent() {
  try {const data=JSON.parse(localStorage.getItem(RECENT_KEY));return Array.isArray(data)?data.slice(0,8):[];} catch {return [];}
}
function rememberReport(data) {
  if(!data?.report_id || !data.report) return;
  recentReports=[{id:data.report_id,filename:data.filename,elapsed_seconds:data.report.elapsed_seconds,openedAt:Date.now()},...recentReports.filter(r=>r.id!==data.report_id)].slice(0,8);
  try {localStorage.setItem(RECENT_KEY,JSON.stringify(recentReports));} catch { /* Storage can be disabled; the current view still works. */ }
  renderRecent();
}
function renderRecent() {
  const html=recentReports.length?recentReports.map(r=>`<a class="recent-report" href="/reports/${encodeURIComponent(r.id)}/report"><span class="recent-file-icon"><svg><use href="#i-report"/></svg></span><span><strong>${safe(r.filename)}</strong><small>Opened recently · ${formatTime(r.elapsed_seconds)}</small></span><svg class="recent-arrow"><use href="#i-arrow"/></svg></a>`).join(''):'<p class="recent-empty">Your completed reports will appear here.</p>';
  $('recent-reports').innerHTML=html;
  $('mobile-recent-reports').innerHTML=html;
  $('recent-count').textContent=recentReports.length?String(recentReports.length).padStart(2,'0'):'';
}
function formatTime(seconds=0) {const n=Math.max(0,Number(seconds)||0);return `${String(Math.floor(n/60)).padStart(2,'0')}:${String(Math.floor(n%60)).padStart(2,'0')}`;}
function size(bytes) {
  if(bytes<1024)return `${bytes} B`;
  if(bytes<1048576)return `${(bytes/1024).toFixed(1)} KiB`;
  if(bytes<1073741824)return `${(bytes/1048576).toFixed(1)} MiB`;
  return `${(bytes/1073741824).toFixed(2)} GB`;
}
async function api(path, options={}) {
  const response=await fetch(path,options);
  const type=response.headers.get('content-type')||'';
  const data=type.includes('json')?await response.json():{error:await response.text()};
  if(!response.ok)throw new Error(data.error||`Request failed (${response.status})`);
  return data;
}
function backend() {return document.querySelector('input[name="backend"]:checked')?.value||'auto';}
function setScanning(running) {
  document.body.classList.toggle('scan-running',running);
  for(const element of [$('file-input'),$('threads'),$('cores-minus'),$('cores-plus'),...document.querySelectorAll('input[name="backend"]')])element.disabled=running;
  document.querySelector('.upload-control').classList.toggle('disabled',running);
  $('upload').disabled=running;
  $('stop').hidden=!running;
  $('start').disabled=running||!selectedName;
  $('start').querySelector('span').textContent=running?'Scanning…':'Start scan';
}
function updateFile(option) {
  selectedName=option?.value||'';selectedSize=Number(option?.dataset.size||0);
  $('selected-file').textContent=selectedName||'No input selected';
  $('file-size').textContent=selectedName?size(selectedSize):'Choose a FASTQ file';
  $('file-format').textContent=selectedName?` · FASTQ${selectedName.endsWith('.gz')?' (gzip)':''}`:'';
  $('start').disabled=!selectedName||Boolean(activeId);
}
function showHit(gene, rsid, ref, alt) {
  const total=ref+alt, percent=total?alt/total*100:0;
  $('hit-detail').innerHTML=`<span class="eyebrow">LIVE SEQUENCE EVIDENCE</span><h2>${safe(gene)} <span class="gene-rsid">${safe(rsid)}</span></h2><div class="hit-pop-stats"><div><span>Reference reads</span><strong>${number.format(ref)}</strong></div><div><span>Alternate reads</span><strong class="${alt?'orange':''}">${number.format(alt)}</strong></div><div><span>Depth</span><strong>${number.format(total)}</strong></div></div><div><span class="eyebrow">ALT SHARE</span><strong class="hit-alt-percent">${percent.toFixed(1)}%</strong><div class="hit-pop-bar"><i style="width:${percent}%"></i></div></div><p class="evidence-note">Live supporting reads from this scan. Open the completed report for genotype, confidence and published findings.</p>`;
  $('hit-dialog').showModal();
}
function updateHits(hits) {
  $('hit-count').textContent=`${hits.length} genes with reads`;
  $('empty-hits').hidden=hits.length>0;
  $('hits').innerHTML=hits.slice(0,12).map((h,i)=>{
    const depth=h.ref_votes+h.alt_votes,altPct=depth?h.alt_votes/depth*100:0;
    return `<button class="hit ${h.alt_votes?'has-alt':''}" data-hit="${i}"><span class="hit-gene">${safe(h.gene)}<small>${safe(h.rsid)}</small></span><span class="hit-evidence"><span class="hit-bar"><i class="ref" style="width:${depth?h.ref_votes/depth*100:0}%"></i><i class="alt" style="width:${altPct}%"></i></span><small>${number.format(h.ref_votes)} / ${number.format(h.alt_votes)} <span>(${altPct.toFixed(1)}% alt)</span></small></span><strong>${compact.format(depth)}</strong></button>`;
  }).join('');
  $('hits').querySelectorAll('[data-hit]').forEach(button=>button.addEventListener('click',()=>{const h=hits[Number(button.dataset.hit)];showHit(h.gene,h.rsid,h.ref_votes,h.alt_votes);}));
  if(helix)helix.setHits(hits);
}
function progressState(snapshot) {
  if(!snapshot)return;
  const elapsed=Math.max(0,snapshot.elapsed_seconds||0);
  const rate=elapsed?snapshot.bytes/elapsed/1e9:0;
  $('rate').innerHTML=`${rate.toFixed(2)}<small>GB/s</small>`;
  $('reads').textContent=compact.format(snapshot.reads||0);$('reads').title=number.format(snapshot.reads||0);
  $('bases').textContent=compact.format(snapshot.bases||0);$('bases').title=number.format(snapshot.bases||0);
  $('elapsed').textContent=formatTime(elapsed);
  const fraction=snapshot.total_bytes?Math.min(snapshot.bytes/snapshot.total_bytes,1):0;
  $('progress-fill').style.width=`${fraction*100}%`;$('progress-percent').innerHTML=`${Math.round(fraction*100)}<span>%</span>`;
  document.querySelector('.progress-track').setAttribute('aria-valuenow',String(Math.round(fraction*100)));
  $('bytes-progress').textContent=`${size(snapshot.bytes)} read${snapshot.total_bytes?' of '+size(snapshot.total_bytes):''}`;
  $('eta').textContent=fraction>0&&fraction<1?`ETA ${formatTime((snapshot.total_bytes-snapshot.bytes)*elapsed/snapshot.bytes)}`:'ETA —';
  $('scan-state').textContent=snapshot.state==='running'?'SCANNING':snapshot.state==='complete'?'COMPLETE':'READY';
  document.body.classList.toggle('scan-running',snapshot.state==='running');
  if(snapshot.state==='running')$('progress-label').textContent='Scanning reads…';else if(snapshot.state==='complete')$('progress-label').textContent='Your report is ready';
  if(helix)helix.update(fraction,hitsLength(snapshot));updateHits(snapshot.hits||[]);
  if(snapshot.result) {
    lastResult=snapshot.result;reportUrl=`/reports/${encodeURIComponent(snapshot.result.report_id)}/report`;
    $('open-report').disabled=false;rememberReport(snapshot.result);
    const gpu=snapshot.result.engine?.gpu_device;
    if(gpu)$('gpu-label').textContent=gpu;
    $('device').textContent=snapshot.result.report.engine;
    $('fallback').textContent=snapshot.result.engine?.fallback_reason||'';
    if(snapshot.state==='complete'&&!completedOnScan&&!location.pathname.startsWith('/reports/')) {
      completedOnScan=true;location.assign(reportUrl);return;
    }
    if(location.pathname.startsWith('/reports/')&&activeId==='show-report')showReport(snapshot.result);
  } else if(snapshot.error) {
    $('progress-label').textContent=snapshot.error;setScanning(false);
  }
}
function hitsLength(snapshot) {return snapshot.hits?.length||0;}
async function scan() {
  if(!selectedName||activeId)return;
  activeId='starting';completedOnScan=false;lastResult=null;reportUrl='';
  setScanning(true);$('open-report').disabled=true;
  $('scan-state').textContent='SCANNING';$('progress-label').textContent='Preparing your sequence…';
  updateHits([]);if(helix){helix.update(0,0);helix.setHits([]);}
  try {
    const created=await api('/api/scans',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({input:selectedName,backend:backend(),threads:Number($('threads').value),gpu_mem_mb:2048})});
    activeId=created.id;reportUrl=created.report_url;
    eventSource=new EventSource(`/api/scans/${activeId}/events`);
    eventSource.addEventListener('progress',event=>{
      const snapshot=JSON.parse(event.data);progressState(snapshot);
      if(snapshot.state!=='running')finishStream();
    });
    eventSource.onerror=()=>{closeStream();pollSnapshot();};
  } catch(error) {activeId='';setScanning(false);$('progress-label').textContent=error.message;$('scan-state').textContent='ERROR';}
}
function closeStream() {if(eventSource){eventSource.close();eventSource=null;}}
function finishStream() {closeStream();activeId='';setScanning(false);}
async function pollSnapshot() {
  if(!activeId||activeId==='starting'||pollTimer)return;
  try {
    const snapshot=await api(`/api/scans/${activeId}`);progressState(snapshot);
    if(snapshot.state!=='running')finishStream();else pollTimer=setTimeout(()=>{pollTimer=null;pollSnapshot();},250);
  } catch(error) {finishStream();$('progress-label').textContent=error.message;}
}
function showReport(data) {
  if(!data?.report)throw new Error('This saved report is not available yet.');
  lastResult=data;activeId='show-report';rememberReport(data);
  $('scan-page').hidden=true;$('report-page').hidden=false;
  document.body.classList.remove('scan-active','scan-running');document.body.classList.add('report-active');
  $('scan-nav').classList.remove('active');
  renderReport(data,$('report-page'));detailNavigation(data);
  detailNavigation(data);
}
window.loadReportById=async id=>{
  activeId='show-report';reportUrl=`/reports/${encodeURIComponent(id)}/report`;
  try {
    const [saved,snapshot]=await Promise.all([api(`/api/scans/${encodeURIComponent(id)}/report.json`),api(`/reports/${encodeURIComponent(id)}`)]);
    // Saved endpoint wraps the JobReport as { report_id, report, filename, ... }.
    const result=saved.report_id&&saved.report.report?{...saved,snapshot}:saved.report_id&&saved.filename?saved:{report_id:id,...snapshot.result};
    showReport(result);
  } catch(error) {
    $('report-page').innerHTML=`<div class="report-error"><span class="eyebrow">REPORT UNAVAILABLE</span><h1>We couldn’t open this report.</h1><p>${safe(error.message)}</p><a class="button primary" href="/">Start a new scan</a></div>`;
    $('scan-page').hidden=true;$('report-page').hidden=false;document.body.classList.add('report-active');
  }
};
function openDialog(id) {closeDetail(false);$(`${id}-dialog`).showModal();document.body.classList.remove('phone-nav-open');}
function goScan() {
  closeDetail(false);$('scan-page').hidden=false;$('report-page').hidden=true;
  document.body.classList.remove('report-active');document.body.classList.add('scan-active');$('scan-nav').classList.add('active');
  if(location.pathname!=='/')history.pushState({},'', '/');else history.replaceState({},'',location.pathname);
}
async function initialize() {
  renderRecent();
  const [files,panel]=await Promise.all([api('/api/files'),api('/api/panel')]);
  $('variant-count').textContent=panel.variants;$('panel-markers').textContent=panel.variants;$('panel-genes').textContent=panel.genes.length;$('panel-traits').textContent=panel.searched_traits;
  $('file-input').innerHTML='<option value="">Choose a FASTQ file…</option>'+files.map(file=>`<option value="${safe(file.name)}" data-size="${file.size}">${safe(file.name)} · ${size(file.size)}</option>`).join('');
  $('file-input').addEventListener('change',()=>updateFile($('file-input').selectedOptions[0]));
  $('file-input').selectedIndex=files.findIndex(file=>file.name==='demo-20gb.fastq')>=0?files.findIndex(file=>file.name==='demo-20gb.fastq')+1:files.length?1:0;
  updateFile($('file-input').selectedOptions[0]);
  $('upload').addEventListener('change',async()=>{
    const file=$('upload').files[0];if(!file)return;
    $('progress-label').textContent=`Uploading ${file.name}…`;
    try {const body=new FormData();body.append('file',file);const uploaded=await api('/api/uploads',{method:'POST',body});$('file-input').insertAdjacentHTML('beforeend',`<option value="${safe(uploaded.name)}" data-size="${uploaded.size}">${safe(file.name)} · ${size(uploaded.size)}</option>`);$('file-input').value=uploaded.name;updateFile($('file-input').selectedOptions[0]);$('progress-label').textContent=`${file.name} is ready`;} catch(error){$('progress-label').textContent=error.message;}
  });
  const stepCores=delta=>{$('threads').value=Math.max(1,Math.min(Number($('threads').max),Number($('threads').value)+delta));$('threads').dispatchEvent(new Event('change'));};
  $('cores-minus').addEventListener('click',()=>stepCores(-1));$('cores-plus').addEventListener('click',()=>stepCores(1));
  $('threads').addEventListener('change',()=>{$('cores-output').textContent=$('threads').value;$('workers').textContent=`${$('threads').value} threads`;drawEngine();});
  $('start').addEventListener('click',scan);$('stop').addEventListener('click',()=>{$('progress-label').textContent='This server does not support cancelling a running scan.';$('gpu-reason').textContent='Stop is unavailable for this scan engine.';});
  $('open-report').addEventListener('click',()=>{if(reportUrl)location.href=reportUrl;});
  $('scan-nav').addEventListener('click',event=>{if(document.body.classList.contains('report-active')){event.preventDefault();goScan();}});
  $('reports-nav').addEventListener('click',()=>{if(innerWidth<=700)openDialog('reports');else document.querySelector('.recent').scrollIntoView({behavior:'smooth',block:'nearest'});});
  $('about-nav').addEventListener('click',()=>openDialog('about'));$('disclaimer-link').addEventListener('click',()=>openDialog('disclaimer'));
  $('disclaimer-text').textContent=DISCLAIMER;
  document.querySelectorAll('dialog').forEach(dialog=>{dialog.querySelector('.dialog-close').addEventListener('click',()=>dialog.close());dialog.addEventListener('click',event=>{if(event.target===dialog){const rect=dialog.getBoundingClientRect();if(event.clientX<rect.left||event.clientX>rect.right||event.clientY<rect.top||event.clientY>rect.bottom)dialog.close();}});});
  $('cores-output').textContent=$('threads').value;
  if(window.pendingReportId){const id=window.pendingReportId;window.pendingReportId=null;await window.loadReportById(id);}
  await drawTelemetry();telemetryTimer=setInterval(async()=>{if(!document.hidden)await drawTelemetry();},1000);
  window.dispatchEvent(new Event('helixscan-ready'));
}
let lastTelemetry=null;
function drawEngine() {
  const t=lastTelemetry;if(!t)return;
  const selected=backend();
  const busy=t.gpu_busy_percent;
  $('gpu-util').textContent=busy===null?'GPU off':`${busy.toFixed(0)}% busy`;
  $('gpu-fill').style.width=`${busy||0}%`;
  $('gpu-reason').textContent=busy===null?(selected==='cpu'?'You chose CPU. No GPU telemetry reported.':'No GPU utilization telemetry reported by this device.'):selected==='cpu'?'GPU telemetry only · you chose CPU.':selected==='gpu'?'GPU selected for the next scan.':'Auto: CPU is used for this file; GPU telemetry is shown for context.';
}
async function drawTelemetry() {
  try {
    const t=await api('/api/telemetry');lastTelemetry=t;logicalCores=t.cpus.length;
    $('simd').textContent=t.simd;$('reader-threads').textContent=`${t.reader_threads} thread`;$('workers').textContent=`${t.workers} threads`;
    $('logical-cores').textContent=`· ${logicalCores||'—'} cores`;
    $('cpu-bars').innerHTML=(t.cpus.length?t.cpus:new Array(12).fill(0)).map((cpu,i)=>`<div title="Core ${i+1}: ${cpu.toFixed(1)}%"><i style="height:${Math.max(cpu,2)}%"></i><small>C${i+1}</small></div>`).join('');
    drawEngine();
  } catch {$('gpu-reason').textContent='Engine telemetry is unavailable.';$('gpu-util').textContent='GPU off';}
}
document.querySelectorAll('input[name="backend"]').forEach(input=>input.addEventListener('change',drawEngine));
const loadHelix=async()=>{try{helix=await createHelix($('helix'));}catch{$('helix').innerHTML='<div class="helix-fallback"></div>';}};
if(document.readyState==='loading')window.addEventListener('DOMContentLoaded',loadHelix,{once:true});else loadHelix();
initialize().catch(error=>{$('progress-label').textContent=error.message;console.error(error);window.dispatchEvent(new Event('helixscan-ready'));});
window.addEventListener('pagehide',()=>{clearInterval(telemetryTimer);clearTimeout(pollTimer);closeStream();disposeHelix();});
export {formatTime,size,number,compact,safe,rememberReport,goScan,api};
// Exposed read-only simulation hooks let the browser check the exact production renderer
// with the project's saved 20 GB simulated scan, without starting a real genome scan.
window.helixscanTest={progressState,setScanning,drawTelemetry};

// Step 2 builds the story here. Keeping its integration explicit avoids racing shared app chrome.
function renderReport(data, container) {
  container.innerHTML='<div class="report-loading"><span class="eyebrow">ANALYSIS COMPLETE</span><h1>Reading your genome…</h1></div>';
}
function detailNavigation() {}
function closeDetail() {}
function initPrint() {}
