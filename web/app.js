import { createHelix, disposeHelix } from './helix.js';

const $ = id => document.getElementById(id) || document.querySelector(id);
const number = new Intl.NumberFormat();
let selectedSize = 0;
let selectedName = '';
let activeId = '';
let lastResult = null;
let eventSource = null;
let lastFinished = '';
let helix = null;
let completedOnScan = false;
let reportUrl = '';
let activePoll = null;
let telemetryTimer = null;

function setHidden(id, hidden) { $(id).hidden = hidden; }
function safe(value) {
  return String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
function size(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KiB`;
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MiB`;
  return `${(bytes / 1073741824).toFixed(2)} GB`;
}
async function api(path, options) {
  const response = await fetch(path, options);
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || `HTTP ${response.status}`);
  return data;
}
function progressState(snapshot) {
  if (!snapshot) return;
  const elapsed = Math.max(snapshot.elapsed_seconds, 0.001);
  const rate = snapshot.bytes / elapsed / 1e9;
  $('rate').textContent = rate.toFixed(3);
  $('reads').textContent = number.format(snapshot.reads);
  $('bases').textContent = number.format(snapshot.bases);
  $('elapsed').textContent = `${snapshot.elapsed_seconds.toFixed(1)} s`;
  $('workers').textContent = `1 / ${snapshot.threads}`;
  const fraction = snapshot.total_bytes ? Math.min(snapshot.bytes / snapshot.total_bytes, 1) : 0;
  $('progress-fill').style.width = `${fraction * 100}%`;
  $('progress-label').textContent = snapshot.state === 'complete' ? 'Scan complete' : `${(fraction * 100).toFixed(1)}% · ${number.format(snapshot.bytes)} / ${number.format(snapshot.total_bytes)} bytes`;
  $('eta').textContent = fraction > 0 && fraction < 1 ? `ETA ${((snapshot.total_bytes - snapshot.bytes) * elapsed / snapshot.bytes).toFixed(1)} s` : '—';
  $('helix-status').className = `status ${snapshot.state}`;
  $('helix-status').innerHTML = `<i></i>${snapshot.state === 'running' ? 'Scanning reads' : snapshot.state === 'complete' ? 'Analysis complete' : 'Scan failed'}`;
  if (helix) helix.update(fraction, snapshot.hits.length);
  const hits = snapshot.hits || [];
  $('hit-count').textContent = `${hits.length} marker${hits.length === 1 ? '' : 's'}`;
  $('empty-hits').hidden = hits.length > 0;
  $('hits').innerHTML = hits.map(hit => {
    const max = Math.max(hit.ref_votes + hit.alt_votes, 1);
    return `<div class="hit"><div class="hit-title"><a href="https://www.ensembl.org/Homo_sapiens/Variation/Explore?v=${encodeURIComponent(hit.rsid)}" target="_blank" rel="noopener">${safe(hit.gene)}</a><small>${safe(hit.rsid)}</small></div><div class="hit-details">${hit.ref_votes} ref / ${hit.alt_votes} alt · ${max} depth</div><div class="hit-bar"><i class="ref" style="width:${hit.ref_votes / max * 100}%"></i><i class="alt" style="width:${hit.alt_votes / max * 100}%"></i></div></div>`;
  }).join('');
  if (hits.length) {
    $('gene-tag').textContent = `${hits[hits.length - 1].gene} · ${hits[hits.length - 1].rsid}`;
    $('gene-tag').classList.add('hit');
    if (helix) helix.setHit(`${hits[hits.length - 1].gene} · ${hits[hits.length - 1].rsid}`);
  }
  if (snapshot.result) {
    $('device').textContent = snapshot.result.engine.gpu_device || snapshot.result.report.engine;
    $('fallback').textContent = snapshot.result.engine.fallback_reason || '';
    lastResult = snapshot.result;
    setHidden('report-tab', false);
    $('open-report').classList.remove('hidden');
    if (snapshot.state === 'complete') {
      $('rate').textContent = (snapshot.result.file_size / snapshot.result.report.elapsed_seconds / 1e9).toFixed(3);
      $('elapsed').textContent = `${snapshot.result.report.elapsed_seconds.toFixed(1)} s`;
    }
    if (activeId === 'show-report' || (!completedOnScan && window.location.pathname.startsWith('/reports/'))) {
      completedOnScan = true;
      lastFinished = snapshot.result.filename;
      showReport(snapshot.result);
    }
  } else if (snapshot.error) {
    $('progress-label').textContent = snapshot.error;
    $('start').disabled = false;
    $('start').textContent = 'Retry scan →';
  }
}
async function scan() {
  if (!selectedName || activeId) return;
  activeId = 'starting';
  $('start').disabled = true;
  $('start').textContent = 'Starting scan…';
  $('progress-label').textContent = 'Preparing panel and input';
  $('fallback').textContent = '';
  lastResult = null;
  completedOnScan = false;
  setHidden('report-tab', true);
  $('gene-tag').textContent = 'Awaiting sequence evidence';
  $('gene-tag').classList.remove('hit');
  if (helix) helix.setHit('');
  $('hits').innerHTML = '';
  $('empty-hits').hidden = false;
  $('helix-status').className = 'status running';
  $('helix-status').innerHTML = '<i></i>Scanning reads';
  try {
    const created = await api('/api/scans', {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({input:selectedName,backend:$('backend').value,threads:Number($('threads').value),gpu_mem_mb:2048})});
    activeId = created.id;
    reportUrl = created.report_url;
    $('start').textContent = 'Scan in progress…';
    eventSource = new EventSource(`/api/scans/${activeId}/events`);
    eventSource.addEventListener('progress', event => {
      const snapshot = JSON.parse(event.data);
      progressState(snapshot);
      if (snapshot.state !== 'running') {
        eventSource.close();
        eventSource = null;
        activeId = '';
        $('start').disabled = false;
        $('start').textContent = 'Scan again →';
      }
    });
    eventSource.onerror = () => {
      if (eventSource) eventSource.close();
      eventSource = null;
      pollSnapshot();
    };
  } catch(error) {
    $('progress-label').textContent = error.message;
    $('start').disabled = !selectedName;
    $('start').textContent = 'Retry scan →';
    activeId = '';
  }
}
async function pollSnapshot() {
  if (!activeId || activeId === 'starting' || activePoll) return;
  try {
    const snapshot = await api(`/api/scans/${activeId}`);
    progressState(snapshot);
    if (snapshot.state !== 'running') {
      activeId = '';
      $('start').disabled = false;
      $('start').textContent = 'Scan again →';
    } else activePoll = setTimeout(() => {activePoll=null;pollSnapshot()}, 250);
  } catch(error) {
    activeId = '';
    $('progress-label').textContent = error.message;
    $('start').disabled = false;
  }
}
function showReport(data) {
  lastResult = data;
  const r = data.report;
  setHidden('scan-page', true);
  setHidden('report-page', false);
  $('report-file').textContent = `${data.filename} · ${size(data.file_size)}`;
  $('report-engine').textContent = `${r.engine} · ${r.elapsed_seconds.toFixed(3)} s${data.engine.gpu_device ? ` · ${data.engine.gpu_device}${data.engine.gpu_driver ? ` · driver ${data.engine.gpu_driver}` : ''}${data.engine.workgroup ? ` · workgroup ${data.engine.workgroup}` : ''}` : ''}${data.engine.fallback_reason ? ` · ${data.engine.fallback_reason}` : ''}`;
  const qc = [
    [number.format(r.qc.reads), 'reads'], [number.format(r.qc.bases), 'bases'],
    [r.qc.mean_read_length.toFixed(1), 'mean bases / read'], [`${r.qc.q30_percent.toFixed(2)}%`, 'Q30 bases'],
    [`${r.qc.gc_percent.toFixed(2)}%`, 'GC; N ' + r.qc.n_percent.toFixed(3) + '%'], [`${r.qc.panel_coverage.toFixed(1)}%`, 'panel coverage']
  ];
  $('qc-cards').innerHTML = qc.map(([value,label]) => `<div class="glass"><strong>${value}</strong><small>${safe(label)}</small></div>`).join('');
  const samples = r.sample_checks;
  $('sample-checks').innerHTML = `<div class="sample-check"><h3>Genetic sex: ${safe(samples.sex.classification)}</h3><p>${safe(samples.sex.evidence)}</p><p>${samples.sex.y_reads_per_million.toFixed(1)} Y marker reads / million; ${samples.sex.control_reads_per_million.toFixed(1)} control / million</p></div><div class="sample-check"><h3>Telomere DNA</h3><p>${samples.telomere.telomeric_reads} evidence reads; ${samples.telomere.reads_per_million.toFixed(1)} / million; index ${samples.telomere.length_adjusted_index.toFixed(3)}.</p><p>${safe(samples.telomere.message)}</p></div><div class="sample-check"><h3>Ancestry similarity</h3><p>${safe(samples.ancestry_summary)}</p>${(()=>{const min=Math.min(...samples.ancestry.map(p=>p.log_likelihood),0);return samples.ancestry.map(p=>`<div class="ancestry-bar" title="${safe(p.population)}: ${p.log_likelihood.toFixed(2)} log likelihood; ${p.markers} markers"><span>${safe(p.population)}</span><div><i style="width:${Math.min(100,(p.log_likelihood-min)*2+2)}%"></i></div><span>${p.markers}</span></div>`).join('')})()}</div>`;
  const groups = [...new Set(r.traits.map(t => t.section))];
  $('section-nav').innerHTML = groups.map(group=>`<a href="#trait-${safe(group).replace(/\s/g,'-')}">${safe(group)}</a>`).join('');
  $('traits').innerHTML = groups.map(group => {
    const traits = r.traits.filter(t => t.section === group);
    return `<div class="glass trait-card" id="trait-${safe(group).replace(/\s/g,'-')}"><h2>${safe(group)}</h2>${traits.map(t=>`<h3>${safe(t.trait_group)}</h3><p>${safe(t.message)}</p>`).join('')}</div>`;
  }).join('');
  $('traits').insertAdjacentHTML('beforeend', `<div class="glass trait-card"><h2>Pharmacogenetic interpretations</h2>${r.pgx.map(p=>`<h3>${safe(p.gene)} · ${safe(p.phenotype)}</h3><p>${safe(p.consult)}</p>`).join('')}</div><div class="glass trait-card"><h2>APOE haplotypes</h2><p>${safe(r.apoe.label)}${r.apoe.haplotypes.length ? ' · ' + safe(r.apoe.haplotypes.join(', ')) : ''}</p></div>`);
  const depths = [...new Map(r.variants.filter(v => v.depth > 0)
    .map(v => [`${v.gene} / ${v.rsid}`,v.depth])).entries()];
  const maxDepth = Math.max(...depths.map(([,depth])=>depth),1);
  $('depth-chart').innerHTML = depths.map(([gene,depth]) =>
    `<div class="depth-item"><span>${safe(gene)}</span><div><i style="width:${depth/maxDepth*100}%"></i></div><strong>${depth}</strong></div>`
  ).join('') || '<p class="no-data">No variants met the minimum coverage.</p>';
  $('variant-rows').innerHTML = r.variants.map(v => {
    const evidence = v.evidence.map(p=>`${safe(p.trait_text)}${p.p_value ? ` · p ${safe(p.p_value)}` : ''}${p.pmid ? ` <a href="https://pubmed.ncbi.nlm.nih.gov/${encodeURIComponent(p.pmid)}/" target="_blank" rel="noopener">PMID ${safe(p.pmid)}</a>` : ''}${p.source ? ` · ${safe(p.source)}` : ''}`).join('<br>');
    return `<tr><td><strong>${safe(v.gene)}</strong><br><small>${safe(v.rsid)}</small></td><td><span class="gt">${safe(v.called ? v.genotype : `No call · ${v.no_call_reason}`)}</span>${v.heteroplasmy !== null ? `<br><small>heteroplasmy ${v.heteroplasmy.toFixed(1)}%</small>` : ''}<br><small>${safe(v.trait_group)}</small></td><td>${v.ref_votes} / ${v.alt_votes.join(', ')}</td><td>${v.depth}</td><td>${v.gq}<div class="gq"><i style="width:${Math.min(v.gq,99)}%"></i></div></td><td><span class="risk">${safe(v.risk_allele || 'Not reported')}</span>${v.dosage !== null ? `<br><small>dosage ${v.dosage}</small>` : ''}</td><td class="evidence">${evidence || '<small>No variant-specific phenotype text returned for this marker.</small>'}</td></tr>`;
  }).join('');
  $('limitations').innerHTML = r.limitations.map(item=>`<li>${safe(item)}</li>`).join('');
}
window.loadReportById = async id => {
  activeId = 'show-report';
  reportUrl = `/reports/${encodeURIComponent(id)}/report`;
  const [result,snapshot] = await Promise.all([
    api(`/reports/${encodeURIComponent(id)}`),
    api(`/reports/${encodeURIComponent(id)}`),
  ]);
  lastResult = result;
  showReport(result);
  progressState(snapshot);
};

async function initialize() {
  const [files, panel] = await Promise.all([api('/api/files'),api('/api/panel')]);
  $('variant-count').textContent = panel.variants;
  $('file-input').innerHTML = '<option value="">Choose a file…</option>' + files.map(file=>`<option value="${safe(file.name)}" data-size="${file.size}">${safe(file.name)}</option>`).join('');
  $('file-input').addEventListener('change', () => {
    const option = $('file-input').selectedOptions[0];
    selectedName = $('file-input').value;
    selectedSize = Number(option.dataset.size || 0);
    $('file-size').textContent = selectedName ? size(selectedSize) : '—';
    $('file-format').textContent = selectedName.endsWith('.gz') ? 'Gzip FASTQ' : selectedName ? 'FASTQ' : '—';
    $('start').disabled = !selectedName || Boolean(activeId);
  });
  $('upload').addEventListener('change', async () => {
    const file = $('upload').files[0];
    if (!file) return;
    $('start').disabled = true;
    $('file-size').textContent = 'Uploading…';
    try {
      const body = new FormData(); body.append('file',file);
      const uploaded = await api('/api/uploads',{method:'POST',body});
      const option = document.createElement('option');
      option.value = uploaded.name; option.dataset.size = uploaded.size; option.textContent = `${file.name} (uploaded)`;
      $('file-input').append(option); $('file-input').value = uploaded.name;
      $('file-input').dispatchEvent(new Event('change'));
      $('progress-label').textContent = `Uploaded ${file.name}`;
    } catch(error) { $('progress-label').textContent = error.message; }
  });
  $('start').addEventListener('click',scan);
  $('reset').addEventListener('click',()=>{
    if(eventSource) eventSource.close();
    activeId = ''; lastResult = null;
    $('start').textContent = 'Start scan →';
    $('start').disabled = false;
    $('progress-label').textContent = 'Ready for a FASTQ input';
    $('report-tab').classList.add('hidden');
    setHidden('report-page',true); setHidden('scan-page',false);
    $('file-input').dispatchEvent(new Event('change'));
  });
  $('report-tab').addEventListener('click',()=>{if(lastResult)window.location.href=`/reports/${encodeURIComponent(lastResult.report_id)}/report`});
  $('open-report').addEventListener('click',()=>{if(reportUrl)window.location.href=reportUrl});
  $('back').addEventListener('click',()=>{setHidden('report-page',true);setHidden('scan-page',false)});
  $('export').addEventListener('click',()=>{
    const blob = new Blob([JSON.stringify(lastResult,null,2)],{type:'application/json'});
    const a = document.createElement('a'); a.href = URL.createObjectURL(blob); a.download = `helixscan-${activeId || 'report'}.json`; a.click(); setTimeout(()=>URL.revokeObjectURL(a.href),10000);
  });
  $('print').addEventListener('click',()=>window.print());
  $('simd').textContent = 'Detecting…';
  const drawTelemetry = async () => {
    try {
      const t = await api('/api/telemetry');
      $('simd').textContent = t.simd;
      $('workers').textContent = `${t.reader_threads} / ${t.workers}`;
      $('gpu-util').textContent = t.gpu_busy_percent === null ? 'Unavailable' : `${t.gpu_busy_percent.toFixed(1)}%`;
      $('cpu-bars').innerHTML = t.cpus.map((cpu,i)=>`<div title="CPU ${i}: ${cpu.toFixed(1)}%"><i style="transform:scaleY(${Math.max(cpu/100,.02)})"></i><small>${i}</small></div>`).join('');
    } catch { $('gpu-util').textContent = 'Unavailable'; }
  };
  if(window.pendingReportId) {
    const id=window.pendingReportId;
    window.pendingReportId=null;
    await window.loadReportById(id);
  }
  await drawTelemetry(); telemetryTimer=setInterval(async()=>{
    if(document.hidden)return;
    await drawTelemetry();
  },1000);
  $('threads').addEventListener('change',()=>{$('workers').textContent=`1 / ${$('threads').value}`});
  window.addEventListener('pagehide',()=>{clearInterval(telemetryTimer);disposeHelix();});
  window.dispatchEvent(new Event('helixscan-ready'));
}

// A graphics failure does not prevent any scan or report operation.
const loadHelix = async () => {
  try { helix = await createHelix($('helix')); }
  catch(error) { $('helix').innerHTML = '<p class="helix-loading">3D view unavailable on this browser. Live scan data remains fully available.</p>'; console.warn('Helix renderer unavailable:',error); }
};
// Load graphics without delaying scan controls or a saved report.
if (document.readyState === 'loading') {
  window.addEventListener('DOMContentLoaded', loadHelix, {once:true});
} else loadHelix();
initialize().catch(error => {
  $('progress-label').textContent = error.message;
  console.error(error);
  window.dispatchEvent(new Event('helixscan-ready'));
});
