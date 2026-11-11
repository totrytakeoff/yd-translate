const invoke = window.__TAURI__ && window.__TAURI__.core
  ? window.__TAURI__.core.invoke
  : null;

const input = document.getElementById('input');
const result = document.getElementById('result');
const meta = document.getElementById('meta');
const dirBtn = document.getElementById('dir');
const copyBtn = document.getElementById('copy');
const speakBtn = document.getElementById('speak');

const mainView = document.getElementById('main-view');
const settingsView = document.getElementById('settings');
const settingsBtn = document.getElementById('settings-btn');
const ocrBtn = document.getElementById('ocr-btn');
const ocrFile = document.getElementById('ocr-file');
const ocrView = document.getElementById('ocr-view');
const dropZone = document.getElementById('drop-zone');
const ocrPreview = document.getElementById('ocr-preview');
const ocrThumb = document.getElementById('ocr-thumb');
const ocrStatus = document.getElementById('ocr-status');
const closeOcrBtn = document.getElementById('close-ocr');
const pasteBtn = document.getElementById('paste-btn');
const closeSettingsBtn = document.getElementById('close-settings');
const saveSettingsBtn = document.getElementById('save-settings');
const toggleSecretBtn = document.getElementById('toggle-secret');
const appKeyInput = document.getElementById('app-key');
const appSecretInput = document.getElementById('app-secret');
const settingsStatus = document.getElementById('settings-status');
const configPath = document.getElementById('config-path');

const DIRECTIONS = {
  en2zh: { from: 'en', to: 'zh-CHS', label: 'EN → 中' },
  zh2en: { from: 'zh-CHS', to: 'en', label: '中 → EN' },
};

let direction = 'en2zh';
let seq = 0;
let debounceTimer = null;
let settingsOpen = false;
let pickingFile = false;
let ocrOpen = false;
let lastResult = null;

const HISTORY_KEY = 'yd.gui.history.v1';
const HISTORY_MAX = 20;
const HISTORY_SHOW = 6;

function el(tag, cls, text) {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text !== undefined && text !== null) node.textContent = text;
  return node;
}

function clear(node) {
  while (node.firstChild) node.removeChild(node.firstChild);
}

function loadHistory() {
  try {
    const raw = localStorage.getItem(HISTORY_KEY);
    const list = raw ? JSON.parse(raw) : [];
    return Array.isArray(list) ? list : [];
  } catch (e) {
    return [];
  }
}

function saveHistory(list) {
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(list.slice(0, HISTORY_MAX)));
  } catch (e) { /* 存储不可用时忽略 */ }
}

function summarize(r) {
  if (r.mode === 'dict' && r.dict) {
    if (r.dict.entries && r.dict.entries.length) return r.dict.entries[0];
    if (r.dict.web && r.dict.web.length) return r.dict.web[0];
    return '';
  }
  return r.translation || '';
}

function recordHistory(r) {
  const item = { q: r.query, s: summarize(r), from: r.from, to: r.to, ts: Date.now() };
  const list = loadHistory().filter(
    x => !(x.q === item.q && x.from === item.from && x.to === item.to)
  );
  list.unshift(item);
  saveHistory(list);
}

function setDirection(name) {
  if (!DIRECTIONS[name]) return;
  direction = name;
  dirBtn.textContent = DIRECTIONS[name].label;
}

function renderHistory() {
  clear(result);
  const list = loadHistory();
  if (!list.length) {
    result.appendChild(el('div', 'hint', '输入单词或句子，自动翻译'));
    return;
  }

  const head = el('div', 'section history-head');
  head.appendChild(el('span', '', '最近'));
  const clearBtn = el('button', 'ghost-small', '清空');
  clearBtn.addEventListener('click', () => {
    saveHistory([]);
    renderHistory();
  });
  head.appendChild(clearBtn);
  result.appendChild(head);

  for (const item of list.slice(0, HISTORY_SHOW)) {
    const row = el('button', 'history-row');
    row.appendChild(el('span', 'history-q', item.q));
    row.appendChild(el('span', 'history-s', item.s || ''));
    row.addEventListener('click', () => {
      setDirection(item.from === 'zh-CHS' ? 'zh2en' : 'en2zh');
      input.value = item.q;
      translateNow();
    });
    result.appendChild(row);
  }
}

function renderIdle() {
  renderHistory();
}

function renderDict(d) {
  clear(result);
  result.appendChild(el('div', 'word', d.word));

  const phon = [];
  if (d.uk) phon.push('英[' + d.uk + ']');
  if (d.us) phon.push('美[' + d.us + ']');
  if (phon.length) result.appendChild(el('div', 'phonetic', phon.join('   ')));

  for (const entry of d.entries || []) {
    result.appendChild(el('div', 'entry', '· ' + entry));
  }
  if (d.forms && d.forms.length) {
    result.appendChild(el('div', 'forms', d.forms.map(f => f.name + ' ' + f.value).join('；')));
  }
  if (d.examples && d.examples.length) {
    result.appendChild(el('div', 'section', '例句'));
    for (const ex of d.examples) {
      result.appendChild(el('div', 'example-src', ex.src));
      if (ex.dst) result.appendChild(el('div', 'example-dst', ex.dst));
    }
  }
  if (d.web && d.web.length) {
    result.appendChild(el('div', 'section', '网络释义'));
    for (const line of d.web) result.appendChild(el('div', 'web', '· ' + line));
  }
}

function renderTranslation(r) {
  clear(result);
  if (r.query) result.appendChild(el('div', 'source', r.query));
  result.appendChild(el('div', 'translation', r.translation));
}

function renderError(message, hint) {
  clear(result);
  result.appendChild(el('div', 'error', message));
  if (hint) result.appendChild(el('div', 'error-hint', hint));
}

async function translateNow() {
  const text = input.value.trim();
  if (!text) {
    renderHistory();
    meta.textContent = '';
    lastResult = null;
    return;
  }
  if (!invoke) {
    renderError('Tauri 运行时未就绪', '请通过 yd-gui 启动');
    return;
  }

  const current = ++seq;
  const dir = DIRECTIONS[direction];
  if (!meta.textContent) result.appendChild(el('div', 'hint', '翻译中…'));

  try {
    const r = await invoke('translate', { text, from: dir.from, to: dir.to });
    if (current !== seq) return;

    if (!r.ok) {
      const hint = (r.error && r.error.code === 'CONFIG')
        ? '点右上角「设置」填入应用ID / 应用密钥'
        : '';
      renderError((r.error && r.error.message) || '翻译失败', hint);
      meta.textContent = '';
      return;
    }

    lastResult = r;
    recordHistory(r);

    if (r.mode === 'dict' && r.dict) renderDict(r.dict);
    else renderTranslation(r);

    const label = {
      'youdao-api': '有道',
      'youdao-dict': '有道词典',
      'google': 'Google',
      'bing': 'Bing',
      'cache': '缓存',
    }[r.source] || r.source;
    meta.textContent = label + ' · ' + r.elapsed_ms + ' ms' + (r.cached ? ' · 缓存' : '');
  } catch (e) {
    if (current !== seq) return;
    renderError('请求失败：' + e, '');
  }
}

function setStatus(text, kind) {
  settingsStatus.textContent = text || '';
  settingsStatus.className = 'status' + (kind ? ' ' + kind : '');
}

async function openSettings() {
  if (ocrOpen) {
    ocrOpen = false;
    ocrView.hidden = true;
  }
  settingsOpen = true;
  mainView.hidden = true;
  settingsView.hidden = false;
  setStatus('');
  try {
    const s = await invoke('get_settings');
    appKeyInput.value = s.app_key || '';
    appSecretInput.value = s.app_secret || '';
    configPath.textContent = s.config_path || '';
    if (s.configured) setStatus('已配置凭证（保存后会重新校验）', 'ok');
  } catch (e) {
    setStatus('读取配置失败：' + e, 'err');
  }
  appKeyInput.focus();
}

function closeSettings() {
  settingsOpen = false;
  settingsView.hidden = true;
  mainView.hidden = false;
  input.focus();
}

async function saveSettings() {
  if (!invoke) return;
  const appKey = appKeyInput.value.trim();
  const appSecret = appSecretInput.value.trim();
  setStatus('保存中…');

  try {
    const path = await invoke('save_settings', { appKey, appSecret });
    configPath.textContent = path;

    if (!appKey || !appSecret) {
      setStatus('已保存。未填凭证时：词典可用，整句走 Bing。', 'warn');
      return;
    }
    const test = await invoke('check_credentials');
    setStatus('✓ 校验通过：hello → ' + test, 'ok');
  } catch (e) {
    setStatus(String(e), 'err');
  }
}

input.addEventListener('input', () => {
  clearTimeout(debounceTimer);
  if (!input.value.trim()) {
    renderHistory();
    meta.textContent = '';
    return;
  }
  debounceTimer = setTimeout(translateNow, 320);
});

input.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    clearTimeout(debounceTimer);
    translateNow();
  }
});

document.addEventListener('keydown', (e) => {
  if (ocrOpen && (e.ctrlKey || e.metaKey) && String(e.key).toLowerCase() === 'v') {
    e.preventDefault();
    runClipboardOcr();
    return;
  }
  if (e.key === 'Escape') {
    e.preventDefault();
    if (pickingFile) return;
    if (ocrOpen) closeOcr();
    else if (settingsOpen) closeSettings();
    else if (invoke) invoke('quit_app');
  }
});

dirBtn.addEventListener('click', () => {
  setDirection(direction === 'en2zh' ? 'zh2en' : 'en2zh');
  if (input.value.trim()) translateNow();
});

copyBtn.addEventListener('click', async () => {
  const text = result.innerText.trim();
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
  } catch (e) {
    const ta = document.createElement('textarea');
    ta.value = text;
    document.body.appendChild(ta);
    ta.select();
    document.execCommand('copy');
    ta.remove();
  }
  copyBtn.textContent = '已复制';
  setTimeout(() => { copyBtn.textContent = '复制'; }, 1200);
});

speakBtn.addEventListener('click', () => {
  const r = lastResult;
  if (!r || !r.ok) return;
  let text;
  let lang;
  if (r.mode === 'dict' && r.dict) {
    text = r.dict.word;
    lang = r.from;
  } else {
    text = r.translation;
    lang = r.to;
  }
  if (!text) return;
  const type = lang && String(lang).startsWith('en') ? 2 : 1;
  const url = 'https://dict.youdao.com/dictvoice?audio='
    + encodeURIComponent(text.slice(0, 500)) + '&type=' + type;
  new Audio(url).play().catch(() => {});
});

function setOcrStatus(text, kind) {
  ocrStatus.textContent = text || '';
  ocrStatus.className = 'status' + (kind ? ' ' + kind : '');
}

function openOcr() {
  if (settingsOpen) closeSettings();
  ocrOpen = true;
  mainView.hidden = true;
  settingsView.hidden = true;
  ocrView.hidden = false;
  ocrPreview.hidden = true;
  ocrThumb.removeAttribute('src');
  setOcrStatus('');
  dropZone.focus();
}

function closeOcr() {
  ocrOpen = false;
  ocrView.hidden = true;
  mainView.hidden = false;
  input.focus();
}

function selectOcrFile() {
  // 系统文件对话框打开期间，Esc 只应取消选择，不能退出程序。
  pickingFile = true;
  ocrFile.value = '';
  ocrFile.click();
}

function handleOcrFile(file) {
  if (!file) return;
  if (file.type && file.type.indexOf('image/') !== 0) {
    setOcrStatus('请选择图片文件', 'err');
    return;
  }
  const reader = new FileReader();
  reader.onload = () => runOcr(String(reader.result || ''));
  reader.onerror = () => setOcrStatus('读取图片失败', 'err');
  reader.readAsDataURL(file);
}

function applyOcrPayload(payload) {
  const r = payload && payload.result;
  if (!r || !r.ok) {
    setOcrStatus((r && r.error && r.error.message) || '识别失败', 'err');
    return;
  }

  closeOcr();
  input.value = (payload && payload.text) || '';
  lastResult = r;
  recordHistory(r);

  if (r.mode === 'dict' && r.dict) renderDict(r.dict);
  else renderTranslation(r);

  const label = {
    'youdao-api': '有道',
    'youdao-dict': '有道词典',
    'bing': 'Bing',
    'google': 'Google',
    'cache': '缓存',
  }[r.source] || r.source;
  meta.textContent = 'OCR · ' + label + ' · ' + r.elapsed_ms + ' ms';
}

async function runOcr(dataUrl) {
  if (!invoke) return;
  ocrPreview.hidden = false;
  ocrThumb.src = dataUrl;
  setOcrStatus('识别中…');

  const dir = DIRECTIONS[direction];
  const ocrLang = direction === 'en2zh' ? 'en' : 'zh-CHS';
  try {
    const payload = await invoke('ocr_translate', {
      image: dataUrl,
      lang: ocrLang,
      to: dir.to,
    });
    applyOcrPayload(payload);
  } catch (e) {
    setOcrStatus('OCR 失败：' + e, 'err');
  }
}

ocrBtn.addEventListener('click', openOcr);
closeOcrBtn.addEventListener('click', closeOcr);
dropZone.addEventListener('click', selectOcrFile);
// contenteditable 只是为了让 WebKit 派发 paste 事件；禁止所有实际输入。
dropZone.addEventListener('beforeinput', (e) => e.preventDefault());

async function runClipboardOcr() {
  if (!invoke) return;

  const dir = DIRECTIONS[direction];
  const ocrLang = direction === 'en2zh' ? 'en' : 'zh-CHS';
  setOcrStatus('读取剪贴板…');

  try {
    const payload = await invoke('ocr_clipboard', { lang: ocrLang, to: dir.to });
    if (!payload) {
      setOcrStatus('剪贴板里没有图片', 'warn');
      return;
    }
    applyOcrPayload(payload);
  } catch (err) {
    setOcrStatus('读取剪贴板失败：' + err, 'err');
  }
}

pasteBtn.addEventListener('click', (e) => {
  e.stopPropagation();
  runClipboardOcr();
});

async function runOcrPath(path) {
  if (!invoke) return;
  setOcrStatus('识别中…');
  const dir = DIRECTIONS[direction];
  const ocrLang = direction === 'en2zh' ? 'en' : 'zh-CHS';
  try {
    const payload = await invoke('ocr_path', { path, lang: ocrLang, to: dir.to });
    applyOcrPayload(payload);
  } catch (e) {
    setOcrStatus('OCR 失败：' + e, 'err');
  }
}

// 拖放：Rust 侧收到窗口拖放事件后通过 eval 调用这里
window.__ydDragEnter = () => {
  if (ocrOpen) dropZone.classList.add('dragging');
};
window.__ydDragLeave = () => {
  dropZone.classList.remove('dragging');
};
window.__ydDragDrop = (paths) => {
  dropZone.classList.remove('dragging');
  const list = Array.isArray(paths) ? paths : [];
  const image = list.find((p) => /\.(png|jpe?g|bmp|webp)$/i.test(p));
  if (!image) {
    setOcrStatus('请拖入图片文件（PNG / JPG / BMP / WEBP）', 'err');
    return;
  }
  runOcrPath(image);
};

['dragenter', 'dragover'].forEach((ev) => {
  dropZone.addEventListener(ev, (e) => {
    e.preventDefault();
    dropZone.classList.add('dragging');
  });
});
['dragleave', 'drop'].forEach((ev) => {
  dropZone.addEventListener(ev, (e) => {
    e.preventDefault();
    dropZone.classList.remove('dragging');
  });
});
dropZone.addEventListener('drop', (e) => {
  const files = e.dataTransfer && e.dataTransfer.files;
  if (files && files.length) handleOcrFile(files[0]);
});

window.addEventListener('focus', () => {
  setTimeout(() => { pickingFile = false; }, 600);
});

ocrFile.addEventListener('change', () => {
  const file = ocrFile.files && ocrFile.files[0];
  if (file) handleOcrFile(file);
});

// 粘贴图片（仅在 OCR 界面；用户主动触发，天然跨平台）
document.addEventListener('paste', (e) => {
  if (!ocrOpen) return;
  const items = (e.clipboardData && e.clipboardData.items) || [];
  for (const item of items) {
    if (item.type && item.type.indexOf('image/') === 0) {
      const file = item.getAsFile();
      if (file) {
        e.preventDefault();
        handleOcrFile(file);
        return;
      }
    }
  }
});

settingsBtn.addEventListener('click', openSettings);
closeSettingsBtn.addEventListener('click', closeSettings);
saveSettingsBtn.addEventListener('click', saveSettings);

toggleSecretBtn.addEventListener('click', () => {
  const hidden = appSecretInput.type === 'password';
  appSecretInput.type = hidden ? 'text' : 'password';
  toggleSecretBtn.textContent = hidden ? '隐藏密钥' : '显示密钥';
});

for (const field of [appKeyInput, appSecretInput]) {
  field.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      saveSettings();
    }
  });
}

renderIdle();
input.focus();
