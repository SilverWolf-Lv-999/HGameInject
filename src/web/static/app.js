"use strict";

const state = {
  processes: [],
  selected: 0,
  view: null,
  attaching: false,
  lastView: 0,
};

const elements = {
  brandSub: document.getElementById("brand-sub"),
  attachChip: document.getElementById("attach-chip"),
  detachButton: document.getElementById("detach-button"),
  scanButton: document.getElementById("scan-button"),
  processFilter: document.getElementById("process-filter"),
  processList: document.getElementById("process-list"),
  processHint: document.getElementById("process-hint"),
  screenIdle: document.getElementById("screen-idle"),
  screenAttaching: document.getElementById("screen-attaching"),
  screenModules: document.getElementById("screen-modules"),
  attachingTitle: document.getElementById("attaching-title"),
  attachingMessage: document.getElementById("attaching-message"),
  targetName: document.getElementById("target-name"),
  targetPath: document.getElementById("target-path"),
  targetRuntime: document.getElementById("target-runtime"),
  targetPid: document.getElementById("target-pid"),
  moduleCount: document.getElementById("module-count"),
  sessionMessage: document.getElementById("session-message"),
  moduleGroups: document.getElementById("module-groups"),
  toast: document.getElementById("toast"),
};

async function api(path, options) {
  const response = await fetch(path, options);
  const text = await response.text();
  let payload = null;
  if (text) {
    try {
      payload = JSON.parse(text);
    } catch (error) {
      payload = null;
    }
  }
  if (!response.ok) {
    const message = payload && payload.error ? payload.error : `请求失败 (${response.status})`;
    throw new Error(message);
  }
  return payload;
}

let toastTimer = 0;
function showToast(message, isError) {
  elements.toast.textContent = message;
  elements.toast.hidden = false;
  elements.toast.className = isError ? "toast error" : "toast";
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    elements.toast.hidden = true;
  }, 4200);
}

function escapeText(text) {
  return String(text == null ? "" : text)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

async function loadProcesses(force) {
  try {
    const payload = force
      ? await api("/api/processes", { method: "POST" })
      : await api("/api/processes");
    state.processes = payload.processes || [];
    renderProcesses();
  } catch (error) {
    elements.processHint.textContent = error.message;
  }
}

function renderProcesses() {
  const filter = elements.processFilter.value.trim().toLowerCase();
  const entries = state.processes.filter((entry) => {
    if (!filter) {
      return true;
    }
    return (
      entry.name.toLowerCase().includes(filter) ||
      entry.title.toLowerCase().includes(filter)
    );
  });
  elements.processHint.textContent =
    entries.length === 0
      ? state.processes.length === 0
        ? "未发现带可见窗口的目标进程。启动游戏后点击扫描。"
        : "没有符合过滤条件的进程。"
      : "";
  elements.processList.innerHTML = entries
    .map((entry) => {
      const selected = entry.pid === state.selected ? " selected" : "";
      const title = entry.title ? escapeText(entry.title) : "（无窗口标题）";
      return (
        `<li class="process-item${selected}" data-pid="${entry.pid}">` +
        `<div class="process-name"><span>${escapeText(entry.name)}</span>` +
        `<span class="process-pid">PID ${entry.pid}</span></div>` +
        `<div class="process-title">${title}</div>` +
        `<div class="process-path">${escapeText(entry.path)}</div>` +
        "</li>"
      );
    })
    .join("");
}

async function attach(pid) {
  state.selected = pid;
  state.attaching = true;
  renderProcesses();
  renderScreen();
  elements.attachingTitle.textContent = `正在加载 PID ${pid}`;
  elements.attachingMessage.textContent = "注入增强模块并建立通道…";
  try {
    await api("/api/attach", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ pid }),
    });
  } catch (error) {
    state.attaching = false;
    renderScreen();
    showToast(error.message, true);
  }
}

async function refreshView() {
  try {
    const view = await api("/api/state");
    state.view = view;
    state.lastView = Date.now();
    if (view.attached) {
      state.selected = view.processId;
    }
    state.attaching = view.attaching;
    renderScreen();
    renderHeader(view);
    if (view.attached) {
      renderModules(view);
    }
  } catch (error) {
    elements.brandSub.textContent = error.message;
  }
}

function renderHeader(view) {
  if (view.attached) {
    elements.attachChip.textContent = `已注入 PID ${view.processId}`;
    elements.attachChip.className = "chip chip-live";
    const runtime = view.runtime ? ` · ${view.runtime}` : "";
    elements.brandSub.textContent = `${view.processName}${runtime} · ${view.modules.length} 个模块`;
    elements.detachButton.hidden = false;
  } else if (view.attaching) {
    elements.attachChip.textContent = "注入中";
    elements.attachChip.className = "chip chip-warn";
    elements.brandSub.textContent = "正在建立通道";
    elements.detachButton.hidden = true;
  } else {
    elements.attachChip.textContent = "未注入";
    elements.attachChip.className = "chip chip-idle";
    elements.brandSub.textContent = view.message || "等待选择目标进程";
    elements.detachButton.hidden = true;
  }
}

function renderScreen() {
  const attached = state.view && state.view.attached;
  elements.screenIdle.hidden = attached || state.attaching;
  elements.screenAttaching.hidden = !state.attaching;
  elements.screenModules.hidden = !attached;
  if (!attached || !state.view) {
    return;
  }
  elements.targetName.textContent = state.view.processName || `PID ${state.view.processId}`;
  elements.targetPath.textContent = state.view.processPath || "";
  elements.targetRuntime.textContent = state.view.runtime || "";
  elements.targetRuntime.hidden = !state.view.runtime;
  elements.targetPid.textContent = `PID ${state.view.processId}`;
  elements.moduleCount.textContent = `${state.view.modules.length} 个模块`;
  if (state.view.message) {
    elements.sessionMessage.textContent = state.view.message;
    elements.sessionMessage.hidden = false;
  } else {
    elements.sessionMessage.hidden = true;
  }
}

function renderModules(view) {
  const groups = new Map();
  for (const module of view.modules) {
    const key = module.category || "Misc";
    if (!groups.has(key)) {
      groups.set(key, []);
    }
    groups.get(key).push(module);
  }
  const openStates = collectOpenStates();
  const html = [];
  for (const [category, modules] of [...groups.entries()].sort((left, right) =>
    left[0].localeCompare(right[0]),
  )) {
    html.push(
      `<section><div class="group-head"><span>${escapeText(category)}</span>` +
        `<span class="group-count">${modules.length}</span></div>` +
        `<div class="module-grid">${modules
          .map((module) => renderModule(module, openStates))
          .join("")}</div></section>`,
    );
  }
  elements.moduleGroups.innerHTML = html.join("");
}

function collectOpenStates() {
  const states = {};
  elements.moduleGroups.querySelectorAll(".module-card").forEach((card) => {
    const body = card.querySelector(".module-body");
    states[card.dataset.name] = body && body.classList.contains("open");
  });
  return states;
}

function renderModule(module, openStates) {
  const enabled = module.enabled ? " enabled" : "";
  const checked = module.enabled ? " checked" : "";
  const fields = renderFields(module);
  const bodyOpen = openStates[module.name] ? " open" : "";
  return (
    `<article class="module-card${enabled}" data-name="${escapeText(module.name)}">` +
    `<div class="module-head"><div><div class="module-title">${escapeText(module.name)}</div>` +
    `<div class="module-description">${escapeText(module.description)}</div></div>` +
    `<label class="switch"><input type="checkbox" data-action="toggle"${checked}>` +
    `<span class="switch-track"></span></label></div>` +
    `<div class="module-foot">` +
    `<button class="button ghost compact" type="button" data-action="expand">配置</button>` +
    `<span class="chip">${escapeText(module.category)}</span>` +
    `<span class="chip">键位 ${module.keybind === 0 ? "无" : "0x" + module.keybind.toString(16).toUpperCase()}</span>` +
    "</div>" +
    `<div class="module-body${bodyOpen}">${fields}</div></article>`
  );
}

function renderFields(module) {
  const fields = [renderKeybindField(module)];
  for (const schema of module.schema) {
    const value = module.values ? module.values[schema.key] : undefined;
    fields.push(renderField(module, schema, value));
  }
  return fields.join("");
}

function renderKeybindField(module) {
  const value = module.keybind ? "0x" + module.keybind.toString(16).toUpperCase() : "";
  return (
    `<div class="field"><span class="field-label">快捷键</span>` +
    `<div class="field-control"><input class="input" type="text" data-kind="keybind" ` +
    `placeholder="0x41" value="${escapeText(value)}"></div></div>`
  );
}

function renderField(module, schema, value) {
  const kind = schema.kind;
  if (kind === "toggle") {
    const checked = value ? " checked" : "";
    return (
      `<div class="field"><span class="field-label">${escapeText(schema.label)}</span>` +
      `<div class="field-control"><label class="switch">` +
      `<input type="checkbox" data-kind="toggle" data-key="${escapeText(schema.key)}"${checked}>` +
      `<span class="switch-track"></span></label></div></div>`
    );
  }
  if (kind === "range") {
    const step = schema.step != null ? schema.step : "any";
    const minimum = schema.min != null ? schema.min : 0;
    const maximum = schema.max != null ? schema.max : 100;
    return (
      `<div class="field"><span class="field-label">${escapeText(schema.label)}</span>` +
      `<div class="field-control">` +
      `<input type="range" data-kind="range" data-key="${escapeText(schema.key)}" ` +
      `min="${minimum}" max="${maximum}" step="${step}" value="${value}">` +
      `<input type="number" data-kind="number" data-key="${escapeText(schema.key)}" ` +
      `min="${minimum}" max="${maximum}" step="${step}" value="${value}"></div></div>`
    );
  }
  if (kind === "select") {
    const options = (schema.choices || [])
      .map(
        (choice) =>
          `<option value="${escapeText(choice)}"${choice === value ? " selected" : ""}>` +
          `${escapeText(choice)}</option>`,
      )
      .join("");
    return (
      `<div class="field"><span class="field-label">${escapeText(schema.label)}</span>` +
      `<div class="field-control"><select data-kind="select" data-key="${escapeText(schema.key)}">` +
      `${options}</select></div></div>`
    );
  }
  return (
    `<div class="field"><span class="field-label">${escapeText(schema.label)}</span>` +
    `<div class="field-control"><input class="input" type="text" data-kind="text" ` +
    `data-key="${escapeText(schema.key)}" value="${escapeText(value)}"></div></div>`
  );
}

async function toggleModule(name, enabled) {
  try {
    await api(`/api/module/${encodeURIComponent(name)}/toggle`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ enabled }),
    });
    await refreshView();
  } catch (error) {
    showToast(error.message, true);
    await refreshView();
  }
}

async function changeConfig(name, payload) {
  try {
    await api(`/api/module/${encodeURIComponent(name)}/config`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    await refreshView();
  } catch (error) {
    showToast(error.message, true);
  }
}

elements.moduleGroups.addEventListener("change", (event) => {
  const target = event.target;
  const card = target.closest(".module-card");
  if (!card) {
    return;
  }
  const name = card.dataset.name;
  if (target.dataset.action === "toggle") {
    toggleModule(name, target.checked);
    return;
  }
  const key = target.dataset.key;
  if (!key) {
    return;
  }
  if (target.dataset.kind === "toggle") {
    changeConfig(name, { [key]: target.checked });
    return;
  }
  if (target.dataset.kind === "range") {
    const number = card.querySelector(`input[data-kind="number"][data-key="${key}"]`);
    if (number) {
      number.value = target.value;
    }
    return;
  }
  if (target.dataset.kind === "number") {
    const range = card.querySelector(`input[data-kind="range"][data-key="${key}"]`);
    if (range && target.value !== "") {
      range.value = target.value;
    }
    changeConfig(name, { [key]: Number(target.value) });
    return;
  }
  if (target.dataset.kind === "select") {
    changeConfig(name, { [key]: target.value });
    return;
  }
  if (target.dataset.kind === "text") {
    changeConfig(name, { [key]: target.value });
  }
});

elements.moduleGroups.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-action='expand']");
  if (!button) {
    return;
  }
  const card = button.closest(".module-card");
  const body = card.querySelector(".module-body");
  body.classList.toggle("open");
});

elements.moduleGroups.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") {
    return;
  }
  const target = event.target;
  if (target.dataset.kind !== "keybind" && target.dataset.kind !== "text") {
    return;
  }
  event.preventDefault();
  const card = target.closest(".module-card");
  if (target.dataset.kind === "keybind") {
    const parsed = parseInt(target.value, 0);
    changeConfig(card.dataset.name, { keybind: Number.isFinite(parsed) ? parsed : 0 });
  } else {
    changeConfig(card.dataset.name, { [target.dataset.key]: target.value });
  }
});

elements.processList.addEventListener("click", (event) => {
  const item = event.target.closest(".process-item");
  if (!item) {
    return;
  }
  attach(Number(item.dataset.pid));
});

elements.processFilter.addEventListener("input", renderProcesses);
elements.scanButton.addEventListener("click", () => loadProcesses(true));
elements.detachButton.addEventListener("click", async () => {
  try {
    await api("/api/detach", { method: "POST" });
    state.view = null;
    state.attaching = false;
    renderScreen();
    renderHeader({ attached: false, attaching: false, message: "已断开" });
  } catch (error) {
    showToast(error.message, true);
  }
});

function tick() {
  if (state.attaching || (state.view && state.view.attached)) {
    refreshView();
  }
  window.setTimeout(tick, 1500);
}

async function boot() {
  await loadProcesses(false);
  await refreshView();
  tick();
}

boot();
