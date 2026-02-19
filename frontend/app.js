// ════════════════════════════════════════════════════════════════
// CONFIG
// ════════════════════════════════════════════════════════════════
const API = 'http://localhost:3000';

// ════════════════════════════════════════════════════════════════
// UTILS
// ════════════════════════════════════════════════════════════════
const el = id => document.getElementById(id);

async function apiFetch(path, options = {}) {
  const url = `${API}${path}`;
  const res = await fetch(url, {
    headers: { 'Content-Type': 'application/json', ...options.headers },
    ...options,
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(body.error || body.message || `HTTP ${res.status}`);
  }
  const ct = res.headers.get('content-type') || '';
  if (ct.includes('text/csv')) return res;   // caller handles .text()
  return res.json();
}

function toast(msg, type = 'info') {
  const container = el('toasts');
  const div = document.createElement('div');
  div.className = `toast toast-${type}`;
  div.textContent = msg;
  container.appendChild(div);
  requestAnimationFrame(() => div.classList.add('show'));
  setTimeout(() => {
    div.classList.remove('show');
    setTimeout(() => div.remove(), 320);
  }, 4000);
}

function esc(str) {
  return String(str ?? '')
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}
function fmtCents(c)    { return `$${(c / 100).toFixed(2)}`; }
function fmtDate(iso)   { return new Date(iso).toLocaleString(); }
function fmtMs(ms)      { return `${ms.toFixed(3)} ms`; }
function fmtUs(us)      { return `${us.toFixed(3)} µs`; }
function fmtNum(n)      { return Number(n).toLocaleString(); }

function setBusy(btnId, busy, label) {
  const b = el(btnId);
  if (!b) return;
  b.disabled = busy;
  if (busy) b.dataset.label = b.textContent;
  b.textContent = busy ? '⏳ Loading…' : (label || b.dataset.label || b.textContent);
}

// ════════════════════════════════════════════════════════════════
// NAVIGATION
// ════════════════════════════════════════════════════════════════
const PAGE_LOADERS = {
  dashboard:   loadDashboard,
  products:    loadProducts,
  devolutions: loadDevolutions,
  benchmark:   loadBenchmarkReport,
  sets:        loadSetStatus,
  metrics:     loadMetrics,
};

function navigate(page) {
  document.querySelectorAll('.page').forEach(p => p.classList.add('hidden'));
  document.querySelectorAll('.nav-link').forEach(l => l.classList.remove('active'));
  const pageEl = el(`page-${page}`);
  const navEl  = document.querySelector(`[data-page="${page}"]`);
  if (pageEl) pageEl.classList.remove('hidden');
  if (navEl)  navEl.classList.add('active');
  if (PAGE_LOADERS[page]) PAGE_LOADERS[page]();
}

// ════════════════════════════════════════════════════════════════
// HEALTH
// ════════════════════════════════════════════════════════════════
async function checkHealth() {
  try {
    const start = Date.now();
    await fetch(`${API}/health`);
    const ms = Date.now() - start;
    el('health-dot').className  = 'dot dot-success';
    el('health-text').textContent = `Online (${ms}ms)`;
    return true;
  } catch {
    el('health-dot').className  = 'dot dot-error';
    el('health-text').textContent = 'Offline';
    return false;
  }
}

// ════════════════════════════════════════════════════════════════
// DASHBOARD
// ════════════════════════════════════════════════════════════════
async function loadDashboard() {
  const online = await checkHealth();
  el('dash-health-val').textContent = online ? '✅' : '❌';
  el('dash-health-sub').textContent = online ? 'Service running' : 'Cannot reach backend';

  // Set sizes
  try {
    const d = await apiFetch('/api/benchmark/sets/status');
    el('dash-hash-size').textContent  = fmtNum(d.sizes.hash_set);
    el('dash-index-size').textContent = fmtNum(d.sizes.index_set);
    el('dash-btree-size').textContent = fmtNum(d.sizes.btree_set);
  } catch {
    ['dash-hash-size','dash-index-size','dash-btree-size'].forEach(id => el(id).textContent = 'N/A');
  }

  // Last benchmark
  try {
    const d = await apiFetch('/api/benchmark/report');
    if (d.report) {
      el('dash-last-bench').innerHTML = `
        <span class="badge badge-success">${fmtDate(d.report.run_at)}</span>
        <span class="badge badge-info">${fmtNum(d.report.product_count)} products</span>
        <span class="badge">🏆 Insert: ${esc(d.report.winner_insert)}</span>
        <span class="badge">🔍 Lookup: ${esc(d.report.winner_lookup)}</span>
        <span class="badge">🔄 Iterate: ${esc(d.report.winner_iterate)}</span>
      `;
    } else {
      el('dash-last-bench').innerHTML = '<span class="text-muted">No benchmark run yet — go to Benchmark to run one.</span>';
    }
  } catch {
    el('dash-last-bench').innerHTML = '<span class="text-muted">Could not load benchmark info.</span>';
  }
}

// ════════════════════════════════════════════════════════════════
// PRODUCTS
// ════════════════════════════════════════════════════════════════
let productState = { offset: 0, limit: 50, total: 0 };

async function loadProducts() {
  const params = new URLSearchParams({ limit: productState.limit, offset: productState.offset });
  const cat = el('filter-category').value;
  const min = el('filter-min-price').value;
  const max = el('filter-max-price').value;
  if (cat) params.set('category', cat);
  if (min) params.set('min_price_cents', Math.round(parseFloat(min) * 100));
  if (max) params.set('max_price_cents', Math.round(parseFloat(max) * 100));

  try {
    const d = await apiFetch(`/api/products?${params}`);
    productState.total = d.count;
    renderProductTable(d.data);
    el('products-count').textContent = `${fmtNum(d.count)} products`;
    const page = Math.floor(productState.offset / productState.limit) + 1;
    const pages = Math.ceil(d.count / productState.limit) || 1;
    el('products-page-info').textContent = `Page ${page} / ${pages}`;
    el('products-prev').disabled = productState.offset === 0;
    el('products-next').disabled = productState.offset + productState.limit >= d.count;
  } catch (e) {
    toast(e.message, 'error');
  }
}

function changePage(dir) {
  productState.offset = Math.max(0, productState.offset + dir * productState.limit);
  loadProducts();
}

function renderProductTable(products) {
  if (!products || products.length === 0) {
    el('products-table-body').innerHTML = '<tr><td colspan="7" class="empty">No products found</td></tr>';
    return;
  }
  el('products-table-body').innerHTML = products.map(p => `
    <tr>
      <td class="mono text-muted">${esc(p.id.slice(0, 8))}…</td>
      <td class="fw-medium">${esc(p.name)}</td>
      <td><span class="badge">${esc(p.category)}</span></td>
      <td>${fmtCents(p.price_cents)}</td>
      <td>${p.quantity}</td>
      <td class="text-muted">${fmtDate(p.created_at)}</td>
      <td class="actions">
        <button class="btn btn-sm btn-ghost" onclick="viewProduct('${p.id}')">View</button>
        <button class="btn btn-sm btn-ghost" onclick="openEditProduct('${p.id}')">Edit</button>
        <button class="btn btn-sm btn-danger-ghost" onclick="deleteProduct('${p.id}')">Delete</button>
      </td>
    </tr>
  `).join('');
}

async function viewProduct(id) {
  try {
    const d = await apiFetch(`/api/products/${id}`);
    const p = d.data;
    const times = d.lookup_times_ns;
    const presence = d.set_presence;
    el('product-detail-content').innerHTML = `
      <div class="detail-grid">
        <div class="detail-field"><label>ID</label><code>${esc(p.id)}</code></div>
        <div class="detail-field"><label>Name</label><span>${esc(p.name)}</span></div>
        <div class="detail-field"><label>Category</label><span class="badge">${esc(p.category)}</span></div>
        <div class="detail-field"><label>Price</label><span>${fmtCents(p.price_cents)}</span></div>
        <div class="detail-field"><label>Quantity</label><span>${p.quantity}</span></div>
        ${p.description ? `<div class="detail-field full-width"><label>Description</label><span>${esc(p.description)}</span></div>` : ''}
        <div class="detail-field"><label>Created</label><span>${fmtDate(p.created_at)}</span></div>
        <div class="detail-field"><label>Updated</label><span>${fmtDate(p.updated_at)}</span></div>
      </div>
      <h4 class="section-label">In-Memory Set Lookups</h4>
      <div class="set-lookup-grid">
        <div class="set-lookup-card ${presence.hash_set ? 'found' : 'notfound'}">
          <div class="set-name">HashSet</div>
          <div class="set-time">${(times.hash_set / 1000).toFixed(2)}µs</div>
          <div class="set-order">Unordered · O(1)</div>
        </div>
        <div class="set-lookup-card ${presence.index_set ? 'found' : 'notfound'}">
          <div class="set-name">IndexSet</div>
          <div class="set-time">${(times.index_set / 1000).toFixed(2)}µs</div>
          <div class="set-order">Insertion order · O(1)</div>
        </div>
        <div class="set-lookup-card ${presence.btree_set ? 'found' : 'notfound'}">
          <div class="set-name">BTreeSet</div>
          <div class="set-time">${(times.btree_set / 1000).toFixed(2)}µs</div>
          <div class="set-order">Sorted by name · O(log n)</div>
        </div>
      </div>
    `;
    openModal('modal-product-detail');
  } catch (e) {
    toast(e.message, 'error');
  }
}

function openCreateProduct() {
  el('product-form').reset();
  el('product-form-id').value = '';
  el('modal-product-title').textContent = 'New Product';
  el('product-form-submit').textContent = 'Create';
  openModal('modal-product');
}

async function openEditProduct(id) {
  try {
    const d = await apiFetch(`/api/products/${id}`);
    const p = d.data;
    el('product-form-id').value  = p.id;
    el('product-name').value     = p.name;
    el('product-desc').value     = p.description || '';
    el('product-price').value    = p.price_cents;
    el('product-quantity').value = p.quantity;
    el('product-category').value = p.category;
    el('modal-product-title').textContent = 'Edit Product';
    el('product-form-submit').textContent = 'Update';
    openModal('modal-product');
  } catch (e) {
    toast(e.message, 'error');
  }
}

async function submitProductForm(e) {
  e.preventDefault();
  const id = el('product-form-id').value;
  const payload = {
    name:        el('product-name').value,
    description: el('product-desc').value || null,
    price_cents: parseInt(el('product-price').value),
    quantity:    parseInt(el('product-quantity').value),
    category:    el('product-category').value,
  };
  try {
    if (id) {
      await apiFetch(`/api/products/${id}`, { method: 'PUT', body: JSON.stringify(payload) });
      toast('Product updated', 'success');
    } else {
      await apiFetch('/api/products', { method: 'POST', body: JSON.stringify(payload) });
      toast('Product created', 'success');
    }
    closeModal('modal-product');
    loadProducts();
  } catch (e) {
    toast(e.message, 'error');
  }
}

async function deleteProduct(id) {
  if (!confirm('Delete this product? This cannot be undone.')) return;
  try {
    await apiFetch(`/api/products/${id}`, { method: 'DELETE' });
    toast('Product deleted', 'success');
    loadProducts();
  } catch (e) {
    toast(e.message, 'error');
  }
}

// ════════════════════════════════════════════════════════════════
// DEVOLUTIONS
// ════════════════════════════════════════════════════════════════
async function loadDevolutions() {
  // Populate product selector
  try {
    const d = await apiFetch('/api/products?limit=500');
    const sel = el('dev-product-id');
    sel.innerHTML = '<option value="">— select product —</option>' +
      d.data.map(p => `<option value="${esc(p.id)}">${esc(p.name)} (${fmtCents(p.price_cents)})</option>`).join('');
  } catch { /* ignore */ }

  try {
    const devs = await apiFetch('/api/devolutions');
    renderDevolutions(devs);
  } catch (e) {
    toast(e.message, 'error');
  }
}

function renderDevolutions(devs) {
  el('dev-count').textContent = `${devs.length} returns`;
  if (!devs.length) {
    el('dev-table-body').innerHTML = '<tr><td colspan="6" class="empty">No devolutions yet</td></tr>';
    return;
  }
  el('dev-table-body').innerHTML = devs.map(d => `
    <tr>
      <td class="mono text-muted">${esc(d.id.slice(0, 8))}…</td>
      <td class="fw-medium">${esc(d.product_name)}</td>
      <td><span class="badge">${esc(d.product_category)}</span></td>
      <td>${d.quantity}</td>
      <td>${esc(d.reason)}</td>
      <td class="text-muted">${fmtDate(d.returned_at)}</td>
    </tr>
  `).join('');
}

async function submitDevolutionForm(e) {
  e.preventDefault();
  const returnedAt = el('dev-returned-at').value;
  const payload = {
    product_id:  el('dev-product-id').value,
    quantity:    parseInt(el('dev-quantity').value),
    reason:      el('dev-reason').value,
    ...(returnedAt ? { returned_at: new Date(returnedAt).toISOString() } : {}),
  };
  try {
    await apiFetch('/api/devolutions', { method: 'POST', body: JSON.stringify(payload) });
    toast('Return recorded', 'success');
    e.target.reset();
    loadDevolutions();
  } catch (e) {
    toast(e.message, 'error');
  }
}

// ════════════════════════════════════════════════════════════════
// SEED
// ════════════════════════════════════════════════════════════════
function setSeedCount(n) {
  el('seed-count-input').value = n;
  el('seed-count-display').textContent = fmtNum(n);
}

async function runSeed() {
  const count = parseInt(el('seed-count-input').value);
  setBusy('seed-btn', true);
  el('seed-result').innerHTML = '';
  try {
    const d = await apiFetch(`/api/seed?count=${count}`, { method: 'POST' });
    el('seed-result').innerHTML = `
      <div class="result-card success">
        <div class="result-row"><span>Seeded</span><strong>${fmtNum(d.seeded)} products</strong></div>
        <div class="result-row"><span>Total in DB</span><strong>${fmtNum(d.total_in_db)}</strong></div>
        <div class="result-row"><span>Seed time</span><strong>${d.seed_time_ms.toFixed(1)} ms</strong></div>
        <div class="result-row"><span>Set sync time</span><strong>${d.set_sync_time_ms.toFixed(1)} ms</strong></div>
      </div>
    `;
    toast(`Seeded ${fmtNum(d.seeded)} products`, 'success');
  } catch (e) {
    toast(e.message, 'error');
  } finally {
    setBusy('seed-btn', false, '🌱 Seed');
  }
}

async function clearAll() {
  if (!confirm('Delete ALL products, devolutions, sets, and metrics? This cannot be undone.')) return;
  setBusy('clear-all-btn', true);
  el('clear-result').innerHTML = '';
  try {
    const d = await apiFetch('/api/reset', { method: 'DELETE' });
    el('clear-result').innerHTML = `
      <div class="result-card success">
        <div class="result-row"><span>Products deleted</span><strong>${fmtNum(d.deleted_products)}</strong></div>
        <div class="result-row"><span>Sets cleared</span><strong>HashSet · IndexSet · BTreeSet</strong></div>
        <div class="result-row"><span>Metrics cleared</span><strong>Yes</strong></div>
      </div>
    `;
    toast('All data cleared', 'success');
  } catch (e) {
    toast(e.message, 'error');
  } finally {
    setBusy('clear-all-btn', false, '🗑 Clear All Data');
  }
}

// ════════════════════════════════════════════════════════════════
// BENCHMARK
// ════════════════════════════════════════════════════════════════
async function runBenchmark() {
  setBusy('bench-run-btn', true);
  try {
    const d = await apiFetch('/api/benchmark/run', { method: 'POST' });
    if (d.report) {
      renderBenchmarkResult(d.report, d);
      toast('Benchmark complete!', 'success');
    } else {
      el('bench-result').innerHTML = `<div class="alert alert-warning">${esc(d.message)}</div>`;
    }
  } catch (e) {
    toast(e.message, 'error');
  } finally {
    setBusy('bench-run-btn', false, '▶ Run Benchmark');
  }
}

async function loadBenchmarkReport() {
  try {
    const d = await apiFetch('/api/benchmark/report');
    if (d.report) {
      renderBenchmarkResult(d.report, d);
    } else {
      el('bench-result').innerHTML = `
        <div class="empty-state">
          <p>No benchmark has been run yet.</p>
          <p style="margin-top:8px">Seed some data first, then click <strong>▶ Run Benchmark</strong>.</p>
        </div>`;
    }
  } catch (e) {
    toast(e.message, 'error');
  }
}

function renderBenchmarkResult(report, raw) {
  const COLORS = {
    'HashSet':                   '#58a6ff',
    'IndexSet (LinkedHashSet)':  '#3fb950',
    'BTreeSet':                  '#a78bfa',
  };

  const maxInsert  = Math.max(...report.results.map(r => r.insert_all.duration_ms))  || 1;
  const maxLookup  = Math.max(...report.results.map(r => r.lookup_hit.duration_us))  || 1;
  const maxIterate = Math.max(...report.results.map(r => r.iterate_all.duration_ms)) || 1;
  const maxRemove  = Math.max(...report.results.map(r => r.remove_half.duration_ms)) || 1;

  const isWinner = (setType, field) => report[`winner_${field}`] === setType;

  const cards = report.results.map(r => {
    const color = COLORS[r.set_type] || 'var(--accent)';
    const pct = (val, max) => `${Math.max(4, (val / max) * 100).toFixed(1)}%`;
    return `
      <div class="bench-card" style="border-top: 3px solid ${color}">
        <div class="bench-card-header">
          <h3 style="color:${color}">${esc(r.set_type)}</h3>
          <div class="bench-winners">
            ${isWinner(r.set_type, 'insert')  ? '<span class="winner-badge">🏆 Insert</span>'  : ''}
            ${isWinner(r.set_type, 'lookup')  ? '<span class="winner-badge">🏆 Lookup</span>'  : ''}
            ${isWinner(r.set_type, 'iterate') ? '<span class="winner-badge">🏆 Iterate</span>' : ''}
          </div>
        </div>
        <p class="bench-desc">${esc(r.description)}</p>
        <div class="bench-metrics">
          ${metricRow('Insert all',   r.insert_all.duration_ms,  pct(r.insert_all.duration_ms, maxInsert),   'ms', color)}
          ${metricRow('Lookup ✓',     r.lookup_hit.duration_us,  pct(r.lookup_hit.duration_us, maxLookup),   'µs', color)}
          ${metricRow('Lookup ✗',     r.lookup_miss.duration_us, pct(r.lookup_miss.duration_us, maxLookup),  'µs', color)}
          ${metricRow('Iterate all',  r.iterate_all.duration_ms, pct(r.iterate_all.duration_ms, maxIterate), 'ms', color)}
          ${metricRow('Remove ½',     r.remove_half.duration_ms, pct(r.remove_half.duration_ms, maxRemove),  'ms', color)}
        </div>
        <div class="bench-order">${r.order_guaranteed ? '✅' : '🔀'} ${esc(r.order_type)}</div>
        <details class="order-sample">
          <summary>Iteration sample (first 10)</summary>
          <ol>${r.iteration_order_sample.map(n => `<li>${esc(n)}</li>`).join('')}</ol>
        </details>
      </div>`;
  }).join('');

  const meta = [
    `${fmtNum(report.product_count)} products`,
    fmtDate(report.run_at),
    raw.db_load_time_ms != null ? `DB load: ${raw.db_load_time_ms.toFixed(1)} ms` : '',
    raw.benchmark_time_ms != null ? `Benchmark: ${raw.benchmark_time_ms.toFixed(1)} ms` : '',
  ].filter(Boolean).join(' · ');

  el('bench-result').innerHTML = `
    <div class="bench-meta">${esc(meta)}</div>
    <div class="bench-cards">${cards}</div>
    ${raw.ascii_table ? `
      <details class="ascii-details">
        <summary>ASCII Table</summary>
        <pre class="ascii">${esc(raw.ascii_table)}</pre>
      </details>` : ''}
  `;
}

function metricRow(label, val, pct, unit, color) {
  return `
    <div class="metric-row">
      <span class="metric-label">${esc(label)}</span>
      <div class="metric-bar-wrap">
        <div class="metric-bar" style="width:${pct}; background:${color}"></div>
      </div>
      <span class="metric-value">${val.toFixed(3)} ${unit}</span>
    </div>`;
}

// ════════════════════════════════════════════════════════════════
// SET INSPECTOR
// ════════════════════════════════════════════════════════════════
async function loadSetStatus() {
  try {
    const d = await apiFetch('/api/benchmark/sets/status');
    renderSetStatus(d);
  } catch (e) {
    toast(e.message, 'error');
  }
}

function renderSetStatus(d) {
  const defs = [
    {
      key: 'hash_set', name: 'HashSet',
      subtitle: '', color: '#58a6ff',
      order: 'Arbitrary — hash-based, not predictable', guaranteed: false,
      size: d.sizes.hash_set,
      items: d.sample_first_5.hash_set.items,
      note:  d.sample_first_5.hash_set.note,
    },
    {
      key: 'index_set', name: 'IndexSet',
      subtitle: 'LinkedHashSet equivalent', color: '#3fb950',
      order: 'Insertion order (FIFO)', guaranteed: true,
      size: d.sizes.index_set,
      items: d.sample_first_5.index_set.items,
      note:  d.sample_first_5.index_set.note,
    },
    {
      key: 'btree_set', name: 'BTreeSet',
      subtitle: '', color: '#a78bfa',
      order: 'Alphabetically sorted by name', guaranteed: true,
      size: d.sizes.btree_set,
      items: d.sample_first_5.btree_set.items,
      note:  d.sample_first_5.btree_set.note,
    },
  ];

  el('sets-content').innerHTML = defs.map(s => `
    <div class="set-card" style="border-top: 3px solid ${s.color}">
      <div class="set-card-header">
        <div>
          <h3 style="color:${s.color}">${s.name}</h3>
          ${s.subtitle ? `<div class="text-muted" style="font-size:12px">${s.subtitle}</div>` : ''}
        </div>
        <span class="size-badge" style="background:${s.color}22; color:${s.color}; border:1px solid ${s.color}44">
          ${fmtNum(s.size)}
        </span>
      </div>
      <div class="order-tag">${s.guaranteed ? '✅' : '🔀'} ${esc(s.order)}</div>
      <div class="set-items">
        ${s.items.length === 0
          ? '<div class="empty">Set is empty — seed data and run a benchmark first</div>'
          : s.items.map((item, i) => `
              <div class="set-item">
                <span class="item-index">${i + 1}</span>
                <span class="item-name">${esc(item.name)}</span>
                <span class="item-id text-muted">${esc(item.id.slice(0, 8))}…</span>
              </div>`).join('')}
      </div>
      <div class="set-note text-muted">${esc(s.note)}</div>
    </div>
  `).join('');
}

// ════════════════════════════════════════════════════════════════
// STRESS TEST
// ════════════════════════════════════════════════════════════════
async function submitStressTest(e) {
  e.preventDefault();
  setBusy('stress-btn', true);
  el('stress-result').innerHTML = '';

  const payload = {
    concurrency:  parseInt(el('stress-concurrency').value),
    ops_per_user: parseInt(el('stress-ops').value),
  };
  const seed = el('stress-seed').value;
  if (seed) payload.seed_count = parseInt(seed);

  try {
    const d = await apiFetch('/api/stress-test', { method: 'POST', body: JSON.stringify(payload) });
    renderStressResult(d.report);
    toast('Stress test complete!', 'success');
  } catch (e) {
    toast(e.message, 'error');
  } finally {
    setBusy('stress-btn', false, '▶ Run Stress Test');
  }
}

function renderStressResult(r) {
  const total = r.reads + r.creates + r.updates + r.deletes;
  const pct = n => total ? ((n / total) * 100).toFixed(1) : '0.0';

  el('stress-result').innerHTML = `
    <div class="stress-grid">
      <div class="stat-card">
        <div class="stat-label">Throughput</div>
        <div class="stat-value accent">${r.ops_per_second.toFixed(1)}</div>
        <div class="stat-sub">ops / second</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">Total Elapsed</div>
        <div class="stat-value">${r.total_elapsed_ms.toFixed(0)} ms</div>
        <div class="stat-sub">${fmtNum(r.total_ops)} total ops</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">Concurrency</div>
        <div class="stat-value">${r.concurrency}</div>
        <div class="stat-sub">${r.ops_per_user} ops / user</div>
      </div>
      <div class="stat-card ${r.errors > 0 ? 'error' : ''}">
        <div class="stat-label">Errors</div>
        <div class="stat-value" style="${r.errors > 0 ? 'color:var(--red)' : ''}">${r.errors}</div>
        <div class="stat-sub">${r.errors === 0 ? 'all clean ✅' : 'check logs ⚠️'}</div>
      </div>
    </div>

    <div class="stress-section">
      <h4>Latency</h4>
      <div class="latency-grid">
        ${['min','avg','p95','p99','max'].map(k => `
          <div class="latency-card">
            <div class="latency-label">${k.toUpperCase()}</div>
            <div class="latency-value">${r[`${k}_latency_ms`].toFixed(2)} ms</div>
          </div>`).join('')}
      </div>
    </div>

    <div class="stress-section">
      <h4>Operations Breakdown</h4>
      <div class="ops-grid">
        ${opCard('op-read',   r.reads,   'Reads',   r.read_avg_ms,   pct(r.reads))}
        ${opCard('op-create', r.creates, 'Creates', r.create_avg_ms, pct(r.creates))}
        ${opCard('op-update', r.updates, 'Updates', r.update_avg_ms, pct(r.updates))}
        ${opCard('op-delete', r.deletes, 'Deletes', r.delete_avg_ms, pct(r.deletes))}
      </div>
    </div>

    <div class="stress-section">
      <h4>In-Memory Set Performance Under Load</h4>
      <div class="set-timing-grid">
        <div class="set-timing-item">
          <span class="set-timing-label">Insert (all creates)</span>
          <span class="set-timing-value">${(r.set_insert_total_ns / 1e6).toFixed(3)} ms</span>
        </div>
        <div class="set-timing-item">
          <span class="set-timing-label">Lookup (all reads)</span>
          <span class="set-timing-value">${(r.set_lookup_total_ns / 1e6).toFixed(3)} ms</span>
        </div>
        <div class="set-timing-item">
          <span class="set-timing-label">Remove (all updates+deletes)</span>
          <span class="set-timing-value">${(r.set_remove_total_ns / 1e6).toFixed(3)} ms</span>
        </div>
      </div>
    </div>

    <details class="ascii-details">
      <summary>ASCII Summary</summary>
      <pre class="ascii">${esc(r.ascii_summary)}</pre>
    </details>
  `;
}

function opCard(cls, count, label, avgMs, pct) {
  return `
    <div class="op-card ${cls}">
      <div class="op-count">${fmtNum(count)}</div>
      <div class="op-label">${label}</div>
      <div class="op-pct">${pct}%</div>
      <div class="op-avg">${avgMs.toFixed(2)} ms avg</div>
    </div>`;
}

// ════════════════════════════════════════════════════════════════
// METRICS
// ════════════════════════════════════════════════════════════════
async function loadMetrics() {
  try {
    const d = await apiFetch('/api/benchmark/export/json');
    renderMetrics(d);
  } catch (e) {
    toast(e.message, 'error');
  }
}

function renderMetrics(d) {
  el('metrics-entry-count').textContent = `${fmtNum(d.entry_count)} entries`;

  if (!d.aggregated || d.aggregated.length === 0) {
    el('metrics-table-body').innerHTML = '<tr><td colspan="8" class="empty">No metrics yet — run a benchmark first</td></tr>';
    el('metrics-ascii').textContent = '';
    return;
  }

  el('metrics-table-body').innerHTML = d.aggregated.map(r => `
    <tr>
      <td>${esc(r.operation)}</td>
      <td><span class="badge">${esc(r.set_type)}</span></td>
      <td>${r.sample_count}</td>
      <td class="mono">${(r.avg_ns / 1000).toFixed(2)}</td>
      <td class="mono">${(r.p50_ns / 1000).toFixed(2)}</td>
      <td class="mono">${(r.p95_ns / 1000).toFixed(2)}</td>
      <td class="mono">${(r.p99_ns / 1000).toFixed(2)}</td>
      <td class="mono">${r.avg_ms.toFixed(4)}</td>
    </tr>
  `).join('');

  el('metrics-ascii').textContent = d.ascii_table;
}

async function downloadCSV() {
  try {
    const res = await apiFetch('/api/benchmark/export/csv');
    const text = await res.text();
    const blob = new Blob([text], { type: 'text/csv' });
    const url  = URL.createObjectURL(blob);
    const a    = document.createElement('a');
    a.href = url;
    a.download = 'benchmark_metrics.csv';
    a.click();
    URL.revokeObjectURL(url);
    toast('CSV downloaded', 'success');
  } catch (e) {
    toast(e.message, 'error');
  }
}

// ════════════════════════════════════════════════════════════════
// MODALS
// ════════════════════════════════════════════════════════════════
function openModal(id) {
  el(id).classList.remove('hidden');
  el('modal-backdrop').classList.remove('hidden');
}
function closeModal(id) {
  el(id).classList.add('hidden');
  // close backdrop only if no other modals are open
  const anyOpen = [...document.querySelectorAll('.modal')].some(m => !m.classList.contains('hidden'));
  if (!anyOpen) el('modal-backdrop').classList.add('hidden');
}

// ════════════════════════════════════════════════════════════════
// INIT
// ════════════════════════════════════════════════════════════════
function init() {
  // Navigation links
  document.querySelectorAll('[data-page]').forEach(link => {
    link.addEventListener('click', e => { e.preventDefault(); navigate(link.dataset.page); });
  });

  // Modal backdrop click closes all modals
  el('modal-backdrop').addEventListener('click', () => {
    document.querySelectorAll('.modal').forEach(m => m.classList.add('hidden'));
    el('modal-backdrop').classList.add('hidden');
  });

  // Forms
  el('product-form').addEventListener('submit', submitProductForm);
  el('dev-form').addEventListener('submit', submitDevolutionForm);
  el('stress-form').addEventListener('submit', submitStressTest);

  // Seed slider
  el('seed-count-input').addEventListener('input', function () {
    el('seed-count-display').textContent = fmtNum(parseInt(this.value));
  });

  // Product filters — reload on change
  ['filter-category', 'filter-min-price', 'filter-max-price', 'filter-limit'].forEach(id => {
    el(id).addEventListener('change', () => {
      productState.offset = 0;
      productState.limit  = parseInt(el('filter-limit').value) || 50;
      loadProducts();
    });
  });

  // Health check every 30 s
  checkHealth();
  setInterval(checkHealth, 30_000);

  // Start on dashboard
  navigate('dashboard');
}

document.addEventListener('DOMContentLoaded', init);
