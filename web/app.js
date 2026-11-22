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
  $('scan-page').style.setProperty('--phone-control-height','0px');
  document.body.classList.toggle('scan-running',running);
  $('scan-page').style.setProperty('--phone-control-height',`${document.querySelector('.control-bar').getBoundingClientRect().height}px`);
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
  document.querySelector('.phone-progress-fill').style.width=`${fraction*100}%`;
  document.querySelector('.phone-progress strong').textContent=`${Math.round(fraction*100)}%`;
  document.querySelector('.phone-progress-label').textContent=snapshot.state==='complete'?'Your report is ready':snapshot.state==='running'?'Scanning reads…':'Progress saved for this scan';
  document.querySelector('.progress-track').setAttribute('aria-valuenow',String(Math.round(fraction*100)));
  document.querySelector('.phone-progress-label').textContent=snapshot.state==='complete'?'Your report is ready':snapshot.state==='running'?'Scanning reads…':'Progress saved for this scan';
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

/* REPORT v2 · a story made only from this report's returned fields. */
let currentReport=null, detailReturnHash='', chapterObserver=null, countObserver=null, chapterDrawerOpen=false;
const chapterNames={ancestry:'Where your DNA comes from',body:'Your body',health:'Your heart, blood & metabolism',medicines:'Your medicines',care:'Carrier & cancer genes',clock:'Your cells’ clock',limits:'What your DNA cannot tell you'};
function effectCopy(v) {
  if(v.effect_allele_status!=='supported')return {label:'Effect allele unclear',dosage:null,verified:false};
  return {label:v.risk_allele||'Not reported',dosage:v.dosage,verified:true};
}
function genotype(v) {return v.called?(v.genotype||'Genotype unavailable'):`No call · ${v.no_call_reason||'insufficient evidence'}`;}
function findingEffect(f) {
  const effect=f.odds_ratio!=null?`OR ${f.odds_ratio}`:f.beta_coefficient!=null?f.beta_coefficient:'';
  return effect?String(effect):'';
}
function pubmed(pmid) {const id=String(pmid||'').replace(/^PMID:/,'');return id?`<a href="https://pubmed.ncbi.nlm.nih.gov/${encodeURIComponent(id)}/" target="_blank" rel="noopener">PMID ${safe(id)} ↗</a>`:'';}
function variantById(id) {return currentReport.report.variants.find(v=>encodeURIComponent(v.rsid)===id.replace(/^variant-/,'')||v.rsid===id);}
function traitCard(t,i) {
  const variants=currentReport.report.variants.filter(v=>v.trait_group===t.trait_group);
  const called=variants.filter(v=>v.called).length;
  const verified=variants.filter(v=>effectCopy(v).verified);
  const dose=verified.reduce((n,v)=>n+(Number(v.dosage)||0),0);
  const pill=t.trait_group.startsWith('Alzheimer')?`APOE ${currentReport.report.apoe.label}`:verified.length?`${dose} effect-allele ${dose===1?'copy':'copies'}`:'Effect alleles unclear';
  const genes=[...new Set(variants.map(v=>v.gene))].slice(0,4).join(' · ');
  return `<button class="story-card trait-story" data-detail="trait-${encodeURIComponent(t.trait_group)}"><span class="card-art" aria-hidden="true">${lineHelix(i)}</span><span class="story-card-top"><span class="story-kind">${safe(t.section==='trait'?'EVERYDAY GENETICS':t.section==='carrier'?'CARRIER PANEL':t.section==='cancer'?'CANCER PANEL':'RESEARCH ASSOCIATION')}</span><span class="context-pill ${verified.length?'has-effect':''}">${safe(pill)}</span></span><h3>${safe(t.trait_group)}</h3><span class="variant-count">${called} of ${variants.length} variants called · ${t.evidence.length} published findings</span><p>${safe(t.message)}</p><span class="card-genes">${safe(genes)}</span><span class="card-link">Read the evidence <svg><use href="#i-arrow"/></svg></span></button>`;
}
function lineHelix(seed) {
  let path1='',path2='';
  for(let y=0;y<=80;y+=4){const x=22+Math.sin((y+seed*9)*.14)*11;path1+=`${y?'L':'M'}${x} ${y} `;path2+=`${y?'L':'M'}${44-x} ${y} `;}
  return `<svg class="thin-helix" viewBox="0 0 44 82"><path d="${path1}"/><path d="${path2}"/>${Array.from({length:7},(_,i)=>{const y=i*12+3,x=22+Math.sin((y+seed*9)*.14)*11;return `<path d="M${x} ${y} ${44-x} ${y}"/>`}).join('')}</svg>`;
}
function sampleStats(r) {
  const t=r.sample_checks.telomere;
  return [
    ['reads','Reads',compact.format(r.qc.reads),'Sequencing reads included in this report.'],
    ['q30','Q30 bases',`${r.qc.q30_pct.toFixed(1)}%`,`${r.qc.q30_pct.toFixed(1)}% of observed bases are reported at Q30. Q30 is a sequencing quality score; it is not a health score.`],
    ['gc','GC content',`${r.qc.gc_pct.toFixed(1)}%`,'The share of observed bases reported as guanine or cytosine.'],
    ['length','Mean read length',`${r.qc.mean_len.toFixed(0)} bp`,'The mean length of sequencing reads observed in this run.'],
    ['n','N bases',`${r.qc.n_pct.toFixed(3)}%`,'The percentage of uncalled bases in the sequencing reads.'],
    ['coverage','Panel coverage',`${r.qc.panel_coverage.toFixed(1)}%`,'Share of this focused marker panel with a reportable call. It does not describe whole-genome coverage.'],
    ['telomere','Telomere index · rough',t.length_adjusted_index.toFixed(3),`${t.message} ${number.format(t.telomeric_reads)} telomeric reads observed.`],
  ].map(([id,label,value,desc],i)=>`<button class="qc-card ${id==='telomere'?'telomere-card':''}" data-detail="stat-${id}" title="${safe(desc)}"><span class="qc-card-label">${label}</span><strong data-count="${id==='telomere'?'':'true'}">${value}</strong><small>${id==='reads'?'SEQUENCED':id==='telomere'?`${number.format(t.telomeric_reads)} evidence reads`:id==='coverage'?`${r.variants.length} FOCUSED MARKERS`:i<2?'RUN QUALITY':'READ COMPOSITION'}</small><svg><use href="#i-arrow"/></svg></button>`).join('');
}
function ancestryBlock(r) {
  const ancestry=r.sample_checks.ancestry;
  if(!ancestry?.length)return '<p class="no-data">No ancestry comparison was returned for this sample.</p>';
  const closest=ancestry[0].population;
  return `<div class="ancestry-layout"><div class="ancestry-orbit" aria-hidden="true"><div class="orbit-lines"></div>${ancestry.map((p,i)=>`<button class="population-node pop-${p.population.toLowerCase()} ${i===0?'closest':''}" data-detail="ancestry-${encodeURIComponent(p.population)}"><i></i><span>${safe(p.population)}<small>${i===0?'CLOSEST MATCH':`−${p.difference.toFixed(1)} LL`}</small></span></button>`).join('')}</div><div class="ancestry-copy"><span class="eyebrow">A ROUGH REFERENCE COMPARISON</span><h3>Closest to <em>${safe(closest)}</em><br>${ancestry.length === 1?'reference marker':'reference groups'}.</h3><p>${safe(r.sample_checks.ancestry_summary)}</p><div class="ancestry-lines">${ancestry.map((p,i)=>`<button data-detail="ancestry-${encodeURIComponent(p.population)}" class="ancestry-line ${i===0?'selected':''}"><span>${safe(p.population)}${i===0?' · closest':''}</span><span><i style="width:${Math.max(4,100-p.difference/Math.max(ancestry[ancestry.length-1].difference,1)*95)}%"></i></span><small>${p.markers} markers</small></button>`).join('')}</div><button class="text-link" data-detail="sample-ancestry">How this comparison works <svg><use href="#i-arrow"/></svg></button></div></div>`;
}
function medicineCard(p,i) {
  const names=p.gene.split('/');
  const traits=currentReport.report.traits.filter(t=>names.some(n=>t.trait_group.toLowerCase().includes(n.toLowerCase())) || (p.gene==='HCP5'&&t.trait_group.startsWith('Abacavir')));
  return `<button class="medicine-card" data-detail="pgx-${i}"><span class="medicine-cap" aria-hidden="true"></span><span class="eyebrow">GENE–MEDICINE CONTEXT</span><span class="medicine-gene">${safe(p.gene)}</span><h3>${safe(p.phenotype)}</h3><p>${safe(p.consult)}</p><span class="med-variants">${traits.length?traits.map(t=>t.rsids.join(' · ')).join(' · '):'Panel interpretation'}</span><span class="card-link">Your medicine evidence <svg><use href="#i-arrow"/></svg></span></button>`;
}
function clockBlock(r) {
  const t=r.sample_checks.telomere;
  const pct=Math.min(100,Math.max(0,t.length_adjusted_index*100));
  return `<div class="clock-layout"><button class="clock-gauge" data-detail="stat-telomere" aria-label="Inspect telomere evidence"><svg viewBox="0 0 240 150" aria-hidden="true"><path class="gauge-track" d="M30 120a90 90 0 0 1 180 0"/><path class="gauge-value" d="M30 120a90 90 0 0 1 180 0" pathLength="100" stroke-dasharray="${pct} 100"/><line x1="120" y1="120" x2="120" y2="36"/><circle cx="120" cy="120" r="7"/></svg><span>Length-adjusted index<strong>${t.length_adjusted_index.toFixed(3)}</strong><small>ROUGH RESEARCH INDEX</small></span></button><div><span class="eyebrow">A SIGNAL, NOT A BIRTHDAY</span><h3>DNA sequence<br>cannot tell your age.</h3><p>${safe(t.message)}</p><dl class="clock-facts"><div><dt>Telomeric reads</dt><dd>${number.format(t.telomeric_reads)}</dd></div><div><dt>Reads per million</dt><dd>${t.reads_per_million.toFixed(2)}</dd></div></dl><button class="text-link" data-detail="sample-telomere">The evidence & its limits <svg><use href="#i-arrow"/></svg></button></div></div>`;
}
function sexBlock(r) {
  const s=r.sample_checks.sex;
  return `<button class="sex-strip" data-detail="sample-sex"><span class="eyebrow">WHOLE-SAMPLE CHECK</span><strong>Genetic-sex marker classification: ${safe(s.classification)}</strong><p>${safe(s.evidence)}</p><span>Evidence and meaning <svg><use href="#i-arrow"/></svg></span></button>`;
}
function renderReport(data,container) {
  currentReport=data;const r=data.report;
  document.title=`HelixScan · ${data.filename}`;
  const traits=r.traits.filter(t=>['health','trait'].includes(t.section));
  const everyday=traits.filter(t=>t.section==='trait');
  const health=traits.filter(t=>t.section==='health');
  const carrier=r.traits.filter(t=>['carrier','cancer'].includes(t.section));
  const topAncestry=r.sample_checks.ancestry?.[0];
  const chapters=[
    ['ancestry',`Similarity is not identity. Explore the ${topAncestry?.markers||'returned'} ancestry markers that inform this rough reference comparison.`,ancestryBlock(r)],
    ['body','Everyday traits, seen through a small set of markers. Select a card for the exact genotype and published evidence.',everyday.length?`<div class="story-grid">${everyday.map((t,i)=>traitCard(t,i)).join('')}</div>`:'<p class="no-data">No everyday trait interpretations were returned.</p>'],
    ['health','Common-variant associations, presented without predictions or diagnoses. The evidence is always one click away.',health.length?`<div class="story-grid">${health.map((t,i)=>traitCard(t,i)).join('')}</div>`:'<p class="no-data">No health trait interpretations were returned.</p>'],
    ['medicines','A conversation starter for you, your doctor and your pharmacist—not an instruction to start, stop or change treatment.',r.pgx?.length?`<div class="medicine-grid">${r.pgx.map(medicineCard).join('')}</div>`:'<p class="no-data">No pharmacogenetic interpretations were returned.</p>'],
    ['care','These focused checks need clinical confirmation and genetic counseling before any health decision.',carrier.length?`<div class="story-grid">${carrier.map((t,i)=>traitCard(t,i)).join('')}</div>`:'<p class="no-data">No carrier or cancer panel interpretations were returned.</p>'],
    ['clock','A rough research measure of telomeric sequence, with its observed evidence and honest limits.',clockBlock(r)],
    ['limits','The most important context is what this small panel cannot establish. Keep these limits beside every result.',`<div class="limits-layout">${sexBlock(r)}<button class="limits-card" data-detail="sample-ancestry"><span class="eyebrow">ANCESTRY · ROUGH</span><h3>Similarity to a reference is not identity.</h3><p>${safe(r.sample_checks.ancestry_summary)}</p><span class="card-link">See the numbers <svg><use href="#i-arrow"/></svg></span></button><div class="limitations-list"><h3>What this cannot tell you</h3><ul>${r.limitations.map(x=>`<li>${safe(x)}</li>`).join('')}</ul></div></div>`]
  ];
  container.innerHTML=`<div class="report-toolbar"><div><span class="eyebrow">YOUR HELIXSCAN REPORT</span><h2>A personal reading, with evidence.</h2></div><div class="report-actions"><button id="back-to-scan" class="button">← New scan</button><button id="export-json" class="button"><svg><use href="#i-download"/></svg>Export JSON</button><button id="print-report" class="button primary"><svg><use href="#i-print"/></svg>Print / PDF</button></div></div>
  <button id="chapter-menu" class="chapter-menu" aria-label="Open chapter navigation" aria-expanded="false" aria-controls="chapter-map"><svg><use href="#i-report"/></svg><span>Chapters</span></button><nav id="chapter-map" class="chapter-map" aria-label="Report chapters"><a href="#report-cover" class="active"><i></i><span>Opening</span></a>${chapters.map(([id,title],i)=>`<a href="#chapter-${id}"><i></i><span>${String(i+1).padStart(2,'0')} ${title}</span></a>`).join('')}</nav>
  <article class="story">
    <section id="report-cover" class="report-cover">
      <div class="cover-art"><canvas id="genome-fingerprint" aria-label="Unique fingerprint drawn from this report's genotypes" width="600" height="600"></canvas><div class="fingerprint-ring"></div><span class="fingerprint-label">YOUR GENOME · IN FOCUS<small>${r.variants.filter(v=>v.called).length} genotype calls shape this fingerprint</small><time datetime="${new Date().toISOString()}">Read ${new Date().toLocaleDateString(undefined,{year:'numeric',month:'long',day:'numeric'})}</time></span></div>
      <div class="cover-copy"><span class="eyebrow">READING YOUR DNA · RESEARCH REPORT</span><h1><span>${readableAmount(r.qc.bases)} pieces</span><br>of your DNA, read in <span>${readableDuration(r.elapsed_seconds)}.</span></h1><p class="cover-intro">A small window into your story. Explore what the evidence says—and what it doesn’t.</p><div class="sample-file"><svg><use href="#i-file"/></svg><div><strong>${safe(data.filename)}</strong><span>${size(data.file_size)} · ${safe(r.engine)}</span></div></div><button class="quality-badge" data-detail="stat-q30"><i></i>${r.qc.q30_pct.toFixed(0)}% of bases Q30</button><p class="cover-context">${safe(topAncestry?.population||'Reference unavailable')} ancestry similarity (rough) · Genetic-sex markers ${safe(r.sample_checks.sex.classification)}</p><p class="cover-disclaimer">${safe(r.disclaimer)}</p><a class="begin-reading" href="#chapter-ancestry">Begin reading <span>↓</span></a></div>
    </section>
    <section class="quality-strip" aria-label="Run quality">${sampleStats(r)}</section>
    ${chapters.map(([id,title,sub,content],i)=>`<section id="chapter-${id}" data-chapter="${id}" class="story-chapter"><header class="chapter-heading"><div class="chapter-number">${String(i+1).padStart(2,'0')}<span>/ 07</span></div><div><span class="eyebrow">CHAPTER ${String(i+1).padStart(2,'0')}</span><h2>${title}</h2><p>${sub}</p></div><svg class="chapter-helix"><use href="#i-helix"/></svg></header>${content}</section>`).join('')}
    <section id="evidence-appendix" class="evidence-appendix"><header class="chapter-heading"><div class="chapter-number">A<span>APPENDIX</span></div><div><span class="eyebrow">EVERY OBSERVATION</span><h2>The evidence, in full.</h2><p>${r.variants.length} variants · ${r.qc.panel_coverage.toFixed(1)}% panel coverage. Select any row to see genotype, reads and all returned findings.</p><button class="text-link" data-detail="read-depth">Read depth per gene <svg><use href="#i-arrow"/></svg></button></div></header><div class="evidence-table-wrap"><table class="evidence-table"><thead><tr><th>Gene / variant</th><th>Genotype</th><th>Ref / alt reads</th><th>Depth</th><th>GQ</th><th>Effect allele</th><th>Published effect</th><th>Source</th></tr></thead><tbody>${r.variants.map(v=>{const effect=effectCopy(v),f=v.evidence[0]||{};return `<tr data-detail="variant-${encodeURIComponent(v.rsid)}" tabindex="0" role="button"><td><strong>${safe(v.gene)}</strong><small>${safe(v.rsid)}</small></td><td><span class="genotype-chip ${effect.verified&&effect.dosage>0?'has-effect':''}">${safe(genotype(v))}</span></td><td>${number.format(v.ref_votes)} / ${number.format(v.alt_votes.reduce((a,b)=>a+b,0))}</td><td>${number.format(v.depth)}</td><td><span>${v.gq}</span><div class="gq-track"><i style="width:${Math.min(v.gq,100)}%"></i></div></td><td><span class="effect-value">${safe(effect.label)}</span><small>${effect.verified?`${effect.dosage??0} copies`:'Not used for dosage'}</small></td><td>${safe(findingEffect(f)||'Not returned')}</td><td>${pubmed(f.pmid)||safe(f.source||'—')}</td></tr>`}).join('')}</tbody></table></div></section>
  </article>
  <aside id="detail-panel" class="detail-panel" aria-label="Evidence details" aria-modal="true" role="dialog" hidden><div class="detail-top"><button id="detail-back" class="button">← Back</button><button id="detail-close" class="detail-close" aria-label="Close evidence details"><svg><use href="#i-close"/></svg></button></div><div id="detail-content"></div></aside><div id="detail-scrim" class="detail-scrim" hidden></div>`;
  $('back-to-scan').addEventListener('click',goScan);
  $('chapter-menu').addEventListener('click',()=>setChapterDrawer(!chapterDrawerOpen));
  document.querySelectorAll('#chapter-map a').forEach(a=>a.addEventListener('click',()=>setChapterDrawer(false)));
  document.addEventListener('click',event=>{if(chapterDrawerOpen&&!event.target.closest('#chapter-map,#chapter-menu'))setChapterDrawer(false);});
  $('export-json').addEventListener('click',()=>{const blob=new Blob([JSON.stringify(data,null,2)],{type:'application/json'}),a=document.createElement('a');a.href=URL.createObjectURL(blob);a.download=`helixscan-${data.report_id}.json`;a.click();setTimeout(()=>URL.revokeObjectURL(a.href),1000);});
  $('print-report').addEventListener('click',()=>window.print());
  $('detail-close').addEventListener('click',()=>closeDetail(true));$('detail-back').addEventListener('click',()=>closeDetail(true));$('detail-scrim').addEventListener('click',()=>closeDetail(true));
  container.querySelectorAll('[data-detail]').forEach(el=>{el.addEventListener('click',event=>{if(event.target.closest('a'))return;openDetail(el.dataset.detail,true);});if(el.getAttribute('role')==='button')el.addEventListener('keydown',e=>{if(['Enter',' '].includes(e.key)){e.preventDefault();openDetail(el.dataset.detail,true);}});});
  drawFingerprint(r.variants);initChapterObservers();
}
function readableAmount(bases) {
  if(bases>=1e9)return `${(bases/1e9).toFixed(2)} billion`;
  if(bases>=1e6)return `${(bases/1e6).toFixed(0)} million`;
  return number.format(bases);
}
function readableDuration(seconds) {return seconds>=60?`${Math.round(seconds/60)} minutes`:`${Math.round(seconds)} seconds`;}
function drawFingerprint(variants) {
  const canvas=document.getElementById('genome-fingerprint');if(!canvas)return;
  const ctx=canvas.getContext('2d'),w=canvas.width,h=canvas.height,cx=w/2,cy=h/2;
  const calls=variants.filter(v=>v.called),palette=['#02f1ef','#00cddd','#5ee3c4','#ec82c1','#faac41','#8294fa'];
  function hash(text){let n=2166136261;for(const c of text)n=Math.imul(n^c.charCodeAt(0),16777619);return n>>>0;}
  ctx.clearRect(0,0,w,h);ctx.save();ctx.translate(cx,cy);
  const bg=ctx.createRadialGradient(0,0,0,0,0,w*.5);bg.addColorStop(0,'rgba(0,180,196,.075)');bg.addColorStop(.7,'rgba(0,160,177,.025)');bg.addColorStop(1,'transparent');ctx.fillStyle=bg;ctx.fillRect(-cx,-cy,w,h);
  ctx.strokeStyle='rgba(1,239,241,.11)';ctx.lineWidth=1;
  for(const radius of [50,105,204,257]){ctx.beginPath();ctx.ellipse(0,0,radius,radius*.85,-.4,0,Math.PI*2);ctx.stroke();}
  const count=Math.max(calls.length,1);
  for(let i=0;i<count;i++) {
    const v=calls[i],seed=hash(`${v.rsid}:${genotype(v)}`),angle=(seed%6283)/1000,turn=angle+(seed%160)/16;
    const color=palette[seed%palette.length],outer=175+(seed%80),inner=105+(seed%55);
    for(let strand=0;strand<2;strand++) {
      ctx.strokeStyle=color;ctx.fillStyle=color;ctx.globalAlpha=.24+(seed%60)/100;ctx.lineWidth=.9+(seed%3)*.4;
      ctx.beginPath();let start,end;
      for(let j=0;j<=30;j++) {
        const y=-outer+j/30*outer*2,phase=j/30*Math.PI*2.4+turn+strand*Math.PI;
        const perspective=.32+.68*(Math.cos(phase)+1)/2,radius=(inner+(outer-inner)*perspective)*(1+(seed%11)/100);
        const x=Math.sin(phase)*radius*.44;
        if(j===0){start={x,y};ctx.moveTo(x,y);}else{end={x,y};ctx.lineTo(x,y);}
      }
      ctx.stroke();ctx.globalAlpha=.8;ctx.beginPath();ctx.arc(start.x,start.y,1.4+seed%2,0,Math.PI*2);ctx.fill();ctx.beginPath();ctx.arc(end.x,end.y,1.4+seed%2,0,Math.PI*2);ctx.fill();
    }
    ctx.globalAlpha=.22;ctx.strokeStyle=color;ctx.beginPath();ctx.moveTo(-inner*.3,-outer+i/count*outer*2);ctx.lineTo(inner*.3,-outer+i/count*outer*2);ctx.stroke();
  }
  ctx.globalAlpha=.2;ctx.strokeStyle='#01eff1';ctx.setLineDash([3,8]);ctx.beginPath();ctx.moveTo(0,-284);ctx.lineTo(0,284);ctx.stroke();ctx.restore();
}

function setChapterDrawer(open) {
  chapterDrawerOpen=Boolean(open&&matchMedia('(max-width:700px)').matches);
  document.body.classList.toggle('chapter-drawer-open',chapterDrawerOpen);
  $('chapter-menu')?.setAttribute('aria-expanded',String(chapterDrawerOpen));
}
function initChapterObservers() {
  chapterObserver?.disconnect();countObserver?.disconnect();
  const links=[...document.querySelectorAll('#chapter-map a')];
  chapterObserver=new IntersectionObserver(entries=>{const visible=entries.filter(e=>e.isIntersecting).sort((a,b)=>b.intersectionRatio-a.intersectionRatio)[0];if(!visible)return;links.forEach(a=>a.classList.toggle('active',a.getAttribute('href')===`#${visible.target.id}`));},{rootMargin:'-10% 0px -50% 0px',threshold:[0,.15,.4]});
  document.querySelectorAll('.report-cover,.story-chapter').forEach(e=>chapterObserver.observe(e));
  const motion=matchMedia('(prefers-reduced-motion: reduce)').matches;
  countObserver=new IntersectionObserver(entries=>entries.forEach(entry=>{if(entry.isIntersecting){if(!motion)animateCount(entry.target);countObserver.unobserve(entry.target);}}),{threshold:.4});
  document.querySelectorAll('[data-count="true"]').forEach(e=>countObserver.observe(e));
}
function animateCount(element) {
  const final=element.textContent;const match=final.match(/[\d,.]+/);if(!match)return;
  const value=Number(match[0].replaceAll(',',''));if(!Number.isFinite(value))return;
  const decimals=(match[0].split('.')[1]||'').length,start=performance.now();
  const tick=time=>{const t=Math.min((time-start)/650,1),n=value*(1-Math.pow(1-t,3));element.textContent=final.replace(match[0],n.toFixed(decimals));if(t<1)requestAnimationFrame(tick);else element.textContent=final;};requestAnimationFrame(tick);
}
function populationFrequencies(v) {
  const freqs=v.population_frequencies;
  if(!freqs||typeof freqs!=='object')return '<div class="frequency-empty">No public frequency is included in this report for this allele.</div>';
  const effect=effectCopy(v);if(!effect.verified||!effect.label)return '<div class="frequency-empty">Effect allele unclear; population dosage comparison is not shown.</div>';
  const allele=effect.label;
  return `<div class="population-select"><label for="frequency-group">1000 Genomes population · rough comparison</label><select id="frequency-group"><option value="ALL">ALL · all groups</option>${['AFR','AMR','EAS','EUR','SAS'].map(g=>`<option value="${g}" ${g===currentReport.report.sample_checks.ancestry?.[0]?.population?'selected':''}>${g}${g===currentReport.report.sample_checks.ancestry?.[0]?.population?' · closest ancestry (rough)':''}</option>`).join('')}</select></div><div id="frequency-bars"></div><p class="frequency-footnote">Hardy–Weinberg genotype-count estimates (p², 2pq, q²), not frequencies of this report’s trait. The source report supplies allele frequencies.</p>`;
}
function updateFrequency(v) {
  const freqs=v.population_frequencies||{},group=document.getElementById('frequency-group')?.value||'ALL',effect=effectCopy(v);
  const row=freqs[group]||freqs;
  const raw=typeof row==='number'?row:row?.[effect.label];
  const f=typeof raw==='number'?raw:typeof raw==='string'?Number(raw):null;
  if(!Number.isFinite(f)||f<0||f>1){document.getElementById('frequency-bars').innerHTML='<div class="frequency-empty">No public frequency is available for this allele and population.</div>';return;}
  const p=f,q=1-f,shares=[q*q,2*p*q,p*p],dose=effect.dosage;
  document.getElementById('frequency-bars').innerHTML=`<p class="frequency-allele">${safe(effect.label)} frequency <strong>${(f*100).toFixed(1)}%</strong></p><div class="share-bar">${shares.map((s,i)=>`<span class="share-${i} ${i===dose?'yours':''}" style="width:${s*100}%"></span>`).join('')}</div><div class="share-labels">${shares.map((s,i)=>`<div class="${i===dose?'yours':''}"><strong>${i} ${i===1?'copy':'copies'}</strong><span>${(s*100).toFixed(1)}%</span></div>`).join('')}</div>${dose!=null?`<p class="where-you-stand">You have ${dose} ${dose===1?'copy':'copies'} of ${safe(effect.label)}. About ${(shares[dose]*100).toFixed(1)}% of people in this reference group are estimated to have the same count.</p>`:'<p class="where-you-stand">Dosage could not be calculated from this genotype.</p>'}`;
}
function findingList(evidence,limit) {
  const sorted=[...evidence].sort((a,b)=>Number.parseFloat(a.p_value||'Infinity')-Number.parseFloat(b.p_value||'Infinity')).slice(0,limit||evidence.length);
  return sorted.length?sorted.map(f=>`<article class="finding"><p>${safe(f.trait_text)}</p><div class="finding-metrics">${f.p_value?`<span>p ${safe(f.p_value)}</span>`:''}${findingEffect(f)?`<span>${safe(findingEffect(f))}</span>`:''}${f.risk_allele?`<span>reported allele ${safe(f.risk_allele)}</span>`:''}</div><div class="finding-source">${pubmed(f.pmid)}${f.source?`<span>${safe(f.source)}</span>`:''}</div></article>`).join(''):'<p class="no-data">No phenotype findings were returned for this marker.</p>';
}
function variantDetail(v,all=true) {
  const effect=effectCopy(v);
  return `<section class="detail-variant"><div class="variant-identity"><span class="gene-name">${safe(v.gene)}</span><span>${safe(v.rsid)}</span></div><div class="variant-genotype"><span>YOUR GENOTYPE</span><strong>${safe(genotype(v))}</strong><span>${safe(v.trait_group)}</span></div><dl class="detail-metrics"><div><dt>Reference reads</dt><dd>${number.format(v.ref_votes)}</dd></div><div><dt>Alternate reads</dt><dd>${number.format(v.alt_votes.reduce((a,b)=>a+b,0))}</dd></div><div><dt>Depth</dt><dd>${number.format(v.depth)}</dd></div><div><dt>Genotype quality</dt><dd>${v.gq}</dd></div>${v.heteroplasmy!=null?`<div><dt>Heteroplasmy</dt><dd>${v.heteroplasmy.toFixed(1)}%</dd></div>`:''}<div><dt>Effect allele</dt><dd>${safe(effect.label)}</dd></div></dl>${effect.verified?`<p class="detail-note">${effect.dosage??0} effect-allele copies counted from the verified returned allele.</p>`:'<p class="detail-note">No supported effect-allele dosage is shown. This prevents a wrong allele from being labelled as a risk allele.</p>'}<details class="frequency-details" open><summary>Where you stand</summary>${populationFrequencies(v)}</details><h3>${v.evidence.length} published ${v.evidence.length===1?'finding':'findings'}</h3>${findingList(v.evidence,all?undefined:3)}</section>`;
}
function traitDetail(group) {
  const trait=currentReport.report.traits.find(t=>encodeURIComponent(t.trait_group)===group.replace(/^trait-/,''));
  if(!trait)return '<p>No such trait was returned.</p>';
  const variants=currentReport.report.variants.filter(v=>v.trait_group===trait.trait_group);
  const dosage=variants.filter(v=>effectCopy(v).verified).reduce((n,v)=>n+(Number(v.dosage)||0),0);
  return `<p>${safe(trait.message)}</p><div class="detail-callout"><strong>${variants.filter(v=>v.called).length} / ${variants.length}</strong><span>variants called</span><strong>${trait.evidence.length}</strong><span>published findings across this trait group</span>${trait.trait_group.startsWith('Alzheimer')?`<strong>APOE ${safe(currentReport.report.apoe.label)}</strong><span>${safe(currentReport.report.apoe.haplotypes.join(' · '))}</span>`:`<strong>${dosage}</strong><span>verified effect-allele copies; unclear calls excluded</span>`}</div>${variants.map(v=>variantDetail(v,false)).join('')}`;
}
function detailContent(id) {
  const r=currentReport.report,s=r.sample_checks;
  if(id.startsWith('variant-')) {const v=variantById(id);return v?{kicker:v.trait_group,title:`${v.gene} · ${v.rsid}`,subtitle:'The observation and every published finding returned for this marker.',html:variantDetail(v)}:{title:'Variant not found',html:''};}
  if(id.startsWith('trait-')) {const t=r.traits.find(t=>id===`trait-${encodeURIComponent(t.trait_group)}`);return {kicker:t?.section==='trait'?'EVERYDAY GENETICS':'TRAIT EVIDENCE',title:t?.trait_group||'Trait',subtitle:'Your calls, the raw read evidence and published findings.',html:traitDetail(id)};}
  if(id.startsWith('pgx-')) {
    const i=Number(id.replace('pgx-','')),p=r.pgx[i];if(!p)return {title:'Medicine evidence not found',html:''};
    const names=p.gene.split('/'),variants=r.variants.filter(v=>p.gene.split('/').some(n=>v.gene===n)||p.gene==='HCP5'&&v.gene==='HCP5');
    return {kicker:'GENE–MEDICINE CONTEXT',title:p.gene,subtitle:p.phenotype,html:`<div class="consult-callout"><h3>Talk with your doctor or pharmacist</h3><p>${safe(p.consult)}</p></div><p class="detail-note">This interpretation comes from the report’s returned pharmacogenetic rules. It is not a treatment recommendation.</p>${variants.length?variants.map(v=>variantDetail(v)).join(''):'<p class="no-data">No individual marker calls were linked to this interpretation.</p>'}`};
  }
  if(id.startsWith('ancestry-')) {
    const pop=s.ancestry.find(p=>id===`ancestry-${encodeURIComponent(p.population)}`);
    return {kicker:'ANCESTRY · ROUGH',title:`${pop?.population||'Population'} reference comparison`,subtitle:`${s.ancestry[0]?.markers||'A small set of'} markers; similarity is not identity.`,html:pop?`<dl class="detail-metrics"><div><dt>Log likelihood</dt><dd>${pop.log_likelihood.toFixed(3)}</dd></div><div><dt>Difference from closest</dt><dd>${pop.difference.toFixed(3)}</dd></div><div><dt>Markers compared</dt><dd>${pop.markers}</dd></div></dl><p>${safe(s.ancestry_summary)}</p><div class="consult-callout"><p>A statistical comparison to reference allele frequencies cannot establish ethnicity, identity or a complete family history.</p></div>`:'<p>No such population was returned.</p>'};
  }
  if(id==='sample-ancestry')return {kicker:'SAMPLE CHECK · ROUGH',title:'Where your DNA comes from',subtitle:'Reference similarity and its limitations.',html:`<p>${safe(s.ancestry_summary)}</p>${(s.ancestry||[]).map(p=>`<dl class="sample-detail-row"><dt>${safe(p.population)}</dt><dd>LL ${p.log_likelihood.toFixed(2)} · Δ ${p.difference.toFixed(2)} · ${p.markers} markers</dd></dl>`).join('')}${r.limitations.map(x=>`<p class="detail-note">${safe(x)}</p>`).join('')}`};
  if(id==='sample-sex')return {kicker:'WHOLE-SAMPLE CHECK',title:`Genetic-sex markers: ${s.sex.classification}`,subtitle:'Observed marker evidence; not a sex or identity determination.',html:`<p>${safe(s.sex.evidence)}</p><dl class="detail-metrics"><div><dt>Y marker reads / million</dt><dd>${s.sex.y_reads_per_million.toFixed(2)}</dd></div><div><dt>Control reads / million</dt><dd>${s.sex.control_reads_per_million.toFixed(2)}</dd></div></dl><div class="consult-callout"><p>${safe(s.sex.evidence)}</p></div>`};
  if(id==='sample-telomere'||id==='stat-telomere')return {kicker:'ROUGH RESEARCH MEASURE',title:'Your cells’ clock',subtitle:'DNA sequence cannot tell your age.',html:`<p>${safe(s.telomere.message)}</p><dl class="detail-metrics"><div><dt>Telomeric reads</dt><dd>${number.format(s.telomere.telomeric_reads)}</dd></div><div><dt>Reads per million</dt><dd>${s.telomere.reads_per_million.toFixed(3)}</dd></div><div><dt>Length-adjusted index</dt><dd>${s.telomere.length_adjusted_index.toFixed(3)}</dd></div></dl><div class="consult-callout"><p>${safe(s.telomere.message)}</p></div>`};
  if(id.startsWith('stat-')) {
    const stat=id.replace('stat-',''),q=r.qc;
    const descriptions={reads:['Reads',number.format(q.reads),'The sequencing reads included in the report. A productive run should return the reads expected from its input.'],q30:['Q30 bases',`${q.q30_pct.toFixed(2)}%`,'Q30 is the reported proportion of bases with a base-call error probability of 1 in 1,000. A common good-run target is at least 80–90% Q30; this does not measure health.'],gc:['GC content',`${q.gc_pct.toFixed(2)}%`,'The proportion of observed guanine and cytosine bases. There is no single universal good range: it depends on the sequenced material and library.'],length:['Mean read length',`${q.mean_len.toFixed(1)} bp`,'The mean length of reads in this file. The sequencing library determines the expected range.'],n:['N bases',`${q.n_pct.toFixed(3)}%`,'Uncalled bases in the reads. Lower is generally better; no universal clinical interpretation applies.'],coverage:['Panel coverage',`${q.panel_coverage.toFixed(1)}%`,'The proportion of this focused marker panel with a reportable genotype. Higher coverage means fewer no-calls, not whole-genome completeness.']};
    const [title,value,desc]=descriptions[stat]||['Run quality','Returned value','Observed sequencing quality for this run.'];
    return {kicker:'UNDERSTANDING YOUR RUN',title,subtitle:'A sequencing statistic, not a health measurement.',html:`<div class="stat-detail-value">${value}</div><p>${safe(desc)}</p><dl class="detail-metrics"><div><dt>Reads</dt><dd>${number.format(q.reads)}</dd></div><div><dt>Bases</dt><dd>${number.format(q.bases)}</dd></div><div><dt>Mean read length</dt><dd>${q.mean_len.toFixed(1)} bp</dd></div><div><dt>Panel coverage</dt><dd>${q.panel_coverage.toFixed(1)}%</dd></div></dl>`};
  }
  if(id==='read-depth') {
    const genes={};for(const v of r.variants)genes[v.gene]=Math.max(genes[v.gene]||0,v.depth);
    const entries=Object.entries(genes).sort((a,b)=>b[1]-a[1]).slice(0,8),max=Math.max(...entries.map(e=>e[1]),1);
    return {kicker:'READ SUPPORT',title:'Read depth per gene',subtitle:'Maximum depth per gene; the eight deepest genes in this report.',html:`<div class="depth-detail">${entries.map(([g,d])=>`<button data-detail-variant="${encodeURIComponent(g)}"><span>${safe(g)}</span><i><b style="width:${d/max*100}%"></b></i><strong>${compact.format(d)}</strong></button>`).join('')}</div><p class="detail-note">Depth is the number of supporting reads, not the effect size or clinical importance.</p>`};
  }
  return {title:'Evidence details',html:'<p>Select a report card to explore its returned evidence.</p>'};
}
function openDetail(id,addHistory=true) {
  if(!currentReport)return;
  const detail=detailContent(id);
  document.getElementById('detail-content').innerHTML=`<span class="eyebrow detail-kicker">${safe(detail.kicker||'THE EVIDENCE, IN CONTEXT')}</span><h2>${safe(detail.title)}</h2><p class="detail-subtitle">${safe(detail.subtitle||'This is the report’s returned evidence. It is not a diagnosis.')}</p>${detail.html}`;
  setChapterDrawer(false);document.getElementById('detail-panel').hidden=false;document.getElementById('detail-scrim').hidden=false;document.body.classList.add('detail-open');
  if(addHistory){detailReturnHash=location.hash;history.pushState({detail:id},'',`#${id}`);}
  document.getElementById('detail-close').focus({preventScroll:true});
  const select=document.getElementById('frequency-group');
  if(select){const v=variantById(id);const draw=()=>updateFrequency(v);select.addEventListener('change',draw);draw();}
  document.querySelectorAll('[data-detail-variant]').forEach(b=>b.addEventListener('click',()=>{const v=currentReport.report.variants.find(v=>v.gene===b.dataset.detailVariant);if(v)openDetail(`variant-${encodeURIComponent(v.rsid)}`,true);}));
}
function closeDetail(updateHistory=true) {
  setChapterDrawer(false);
  const panel=document.getElementById('detail-panel');if(!panel)return;
  const wasOpen=!panel.hidden;panel.hidden=true;document.getElementById('detail-scrim').hidden=true;document.body.classList.remove('detail-open');
  if(wasOpen&&updateHistory){if(history.state?.detail)history.back();else if(location.hash)history.replaceState({},'',location.pathname);}
}
function detailNavigation() {
  if(location.hash&&!location.hash.startsWith('#chapter-')&&location.hash!=='#report-cover')openDetail(decodeURIComponent(location.hash.slice(1)),false);
  window.onpopstate=()=>{if(location.hash&&!location.hash.startsWith('#chapter-')&&location.hash!=='#report-cover')openDetail(decodeURIComponent(location.hash.slice(1)),false);else closeDetail(false);};
  window.onkeydown=event=>{if(event.key==='Escape'){if(document.body.classList.contains('detail-open'))closeDetail(true);else setChapterDrawer(false);}};
}
