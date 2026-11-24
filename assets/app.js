// Vela Studio client. Vanilla JavaScript, no dependencies, no network access.
(() => {
  "use strict";

  const token = new URLSearchParams(location.search).get("token") || "";
  const $ = (id) => document.getElementById(id);
  const state = { value: null, maxStep: 0, tab: "locals", sourceDirty: false, memoryStart: 0 };

  function esc(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({
      "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
    }[c]));
  }

  async function api(path, body) {
    try {
      const res = await fetch(path, {
        method: body === undefined ? "GET" : "POST",
        headers: { "Content-Type": "application/json", "X-Vela-Token": token },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      let data;
      try { data = await res.json(); } catch { data = { error: `invalid response (${res.status})` }; }
      if (data.error) showError(data.error);
      return data;
    } catch (e) {
      showError(`request failed: ${e.message}`);
      return { error: String(e.message) };
    }
  }

  function showError(message) {
    const box = $("error-box");
    box.textContent = message;
    box.classList.remove("hidden");
  }

  function clearError() {
    $("error-box").classList.add("hidden");
    $("error-box").textContent = "";
  }

  function statusOf(s) {
    if (!s) return ["idle", "idle"];
    if (s.error) return ["trapped", "error"];
    if (s.trap) return ["trapped", "trapped"];
    if (s.finished) return ["finished", "finished"];
    if (rootMessage().startsWith("breakpoint")) return ["paused", "paused"];
    if (s.loaded) return ["ready", "ready"];
    return ["idle", "idle"];
  }

  function rootMessage() {
    return state.value ? String(state.value.message || "") : "";
  }

  function render(s) {
    if (!s) return;
    state.value = s;
    if (s.source && !state.sourceDirty) $("source").value = s.source;
    if (Array.isArray(s.args) && s.args.length) $("args-input").value = s.args.join(", ");

    const [pillClass, pillText] = statusOf(s);
    const pill = $("status-pill");
    pill.className = `pill ${pillClass}`;
    pill.textContent = pillText;
    $("message").textContent = s.error ? s.error : (s.message || "");
    $("step-badge").textContent = `step ${s.step || 0}`;
    $("hash-badge").textContent = s.trace_hash
      ? `t${s.trace_hash.slice(0, 8)} · s${(s.state_hash || "").slice(0, 8)}`
      : "—";
    $("footer-hashes").textContent = s.trace_hash
      ? `trace ${s.trace_hash} · state ${s.state_hash}`
      : "trace — · state —";

    syncExports(s);
    renderDisasm(s);
    renderTab(s);

    const canBack = !!s.can_back;
    $("btn-back").disabled = !canBack;
    $("btn-step").disabled = !s.loaded || !!s.finished || !!s.trap;
    $("btn-run").textContent = rootMessage().startsWith("breakpoint") ? "Resume" : "Run";
    $("btn-finish").disabled = !s.loaded || !!s.finished || !!s.trap;

    if (s.loaded) {
      state.maxStep = Math.max(state.maxStep, s.step || 0, s.trace_total || 0);
    }
    const timeline = $("timeline");
    timeline.max = String(Math.max(state.maxStep, 1));
    timeline.value = String(s.step || 0);
    $("timeline-meta").textContent =
      `${s.step || 0} / ${state.maxStep} steps · ${s.checkpoints || 0} checkpoints · ${s.call_depth || 0} frames`;

    renderBreakpoints(s);
    renderTrace(s);
    if (s.error) showError(s.error); else clearError();
  }

  function syncExports(s) {
    const select = $("export-select");
    const current = select.value;
    const funcs = (s.exports || []).filter((e) => e.kind === "func").map((e) => e.name);
    const known = Array.from(select.options).map((o) => o.value);
    if (funcs.join("|") !== known.join("|")) {
      select.innerHTML = funcs.map((n) => `<option value="${esc(n)}">${esc(n)}</option>`).join("");
    }
    const chosen = funcs.includes(current) ? current : (s.export || funcs[0] || "");
    if (chosen) select.value = chosen;
  }

  function renderDisasm(s) {
    const box = $("disasm");
    $("func-label").textContent = s.current_func_name ? `${s.current_func_name} @${s.current_func}` : "—";
    $("pc-label").textContent = s.pc === null || s.pc === undefined ? "pc —" : `pc ${s.pc}`;
    const rows = s.disasm || [];
    if (!rows.length) {
      box.innerHTML = `<div class="empty">No function loaded.</div>`;
      return;
    }
    const bps = new Set(
      (s.breakpoints || [])
        .filter((b) => b.kind === "instruction" && b.func === s.current_func)
        .map((b) => b.offset)
    );
    box.innerHTML = rows.map((r) => {
      const cls = ["disasm-row"];
      if (r.current) cls.push("current");
      if (bps.has(r.offset)) cls.push("bp");
      return `<div class="${cls.join(" ")}" data-offset="${r.offset}" data-func="${s.current_func}">
        <span class="off">${String(r.offset).padStart(3, "0")}</span>
        <span class="op">${esc(r.text)}</span>
        <span class="bp-dot ${bps.has(r.offset) ? "" : "off"}">●</span>
      </div>`;
    }).join("");
    const current = box.querySelector(".current");
    if (current) current.scrollIntoView({ block: "nearest" });
  }

  function table(headers, rows) {
    if (!rows.length) return `<div class="empty">empty</div>`;
    return `<table class="grid"><thead><tr>${headers.map((h) => `<th>${esc(h)}</th>`).join("")}</tr></thead>
      <tbody>${rows.join("")}</tbody></table>`;
  }

  function renderTab(s) {
    if (state.tab === "locals") {
      const rows = (s.locals || []).map((l) =>
        `<tr><td class="num">${l.index}</td><td class="type">${esc(l.type)}</td><td class="val">${esc(l.value.display)}</td></tr>`);
      $("tab-locals").innerHTML = table(["#", "type", "value"], rows);
    } else if (state.tab === "stack") {
      const rows = (s.stack || []).map((v, i) =>
        `<tr><td class="num">${i}</td><td class="type">${esc(v.type)}</td><td class="val">${esc(v.display)}</td></tr>`);
      $("tab-stack").innerHTML = table(["top", "type", "value"], rows);
    } else if (state.tab === "frames") {
      const rows = (s.frames || []).slice().reverse().map((f) =>
        `<tr><td class="num">${f.depth}</td><td class="val">${esc(f.name)}</td><td class="type">@${f.func}</td><td class="num">pc ${f.offset}</td></tr>`);
      $("tab-frames").innerHTML = rows.length
        ? `<table class="grid"><thead><tr><th>depth</th><th>function</th><th>addr</th><th>pc</th></tr></thead><tbody>${rows.join("")}</tbody></table>`
        : `<div class="empty">no frames</div>`;
    } else if (state.tab === "globals") {
      const rows = (s.globals || []).map((g) =>
        `<tr><td class="num">${g.index}</td><td class="type">${esc(g.type)}${g.mutable ? " mut" : ""}</td><td class="val">${esc(g.value.display)}</td></tr>`);
      $("tab-globals").innerHTML = table(["#", "type", "value"], rows);
    } else if (state.tab === "memory") {
      renderMemory(s);
    }
  }

  function renderMemory(s) {
    const box = $("tab-memory");
    if (!s.memory) {
      box.innerHTML = `<div class="empty">no memory</div>`;
      return;
    }
    const m = s.memory;
    const data = m.data || "";
    const lines = [];
    for (let i = 0; i < data.length; i += 32) {
      const chunk = data.slice(i, i + 32);
      const bytes = chunk.match(/.{1,2}/g) || [];
      const hex = bytes.map((b) => `<span class="byte${b === "00" ? " zero" : ""}">${b}</span>`).join(" ");
      const ascii = bytes.map((b) => {
        const v = parseInt(b, 16);
        return v >= 32 && v < 127 ? esc(String.fromCharCode(v)) : ".";
      }).join("");
      const addr = (m.start + i / 2).toString(16).padStart(8, "0");
      lines.push(`<span class="addr">${addr}</span>  ${hex}  <span class="ascii">${ascii}</span>`);
    }
    box.innerHTML = `
      <div class="memory-toolbar">
        <span class="subtle">${m.pages} pages · ${m.bytes} bytes</span>
        <input id="mem-start" value="${m.start}" inputmode="numeric" aria-label="memory window start">
        <button id="btn-mem-go">Go</button>
      </div>
      <div class="hex">${lines.join("\n")}</div>`;
    const go = $("btn-mem-go");
    if (go) {
      go.onclick = () => {
        const v = parseInt($("mem-start").value, 10);
        if (Number.isFinite(v) && v >= 0) {
          state.memoryStart = v;
          fetchState(v);
        }
      };
    }
  }

  function renderBreakpoints(s) {
    const list = $("bp-list");
    const bps = s.breakpoints || [];
    $("bp-host").checked = bps.some((b) => b.kind === "host");
    if (!bps.length) {
      list.innerHTML = `<li><span class="subtle">none</span></li>`;
      return;
    }
    list.innerHTML = bps.map((b, i) => {
      let label = b.kind;
      if (b.kind === "instruction") label = `instr @${b.func}:${b.offset}`;
      if (b.kind === "function") label = `func @${b.func}`;
      if (b.kind === "memory") label = `mem 0x${Number(b.address).toString(16)}`;
      return `<li data-index="${i}" title="click to remove"><span>${esc(label)}</span><span class="tag">●</span></li>`;
    }).join("");
  }

  function renderTrace(s) {
    const box = $("trace-list");
    const events = s.trace || [];
    if (!events.length) {
      box.innerHTML = `<div class="empty">No instructions executed yet. Press Run or Step.</div>`;
      return;
    }
    const slice = events.slice(-60);
    box.innerHTML = slice.map((e) => {
      const current = e.step === (s.step || 0) - 1;
      return `<div class="trace-row ${current ? "current" : ""}">
        <span class="step">${e.step}</span>
        <span>${esc(e.name || `func@${e.func}`)}</span>
        <span>${e.offset}</span>
        <span class="op">${esc(e.op)}</span>
        <span>${e.stack}</span>
        <span>${e.depth}</span>
      </div>`;
    }).join("");
    box.scrollTop = box.scrollHeight;
  }

  async function fetchState(start) {
    const q = start !== undefined ? `?start=${start}` : "";
    const s = await api(`/api/state${q}`);
    if (!s.error) render(s);
    else if (s.state) render(s.state);
  }

  async function action(name, target) {
    const data = await api("/api/action", { action: name, target });
    if (data.error && data.state) { render(data.state); return; }
    if (!data.error) {
      if (name === "restart") state.maxStep = 0;
      render(data);
    }
  }

  function parseArgs(text) {
    return text.split(/[\s,]+/).filter(Boolean).map((t) => {
      const v = parseInt(t, 10);
      return Number.isFinite(v) ? v : 0;
    });
  }

  async function load() {
    clearError();
    state.sourceDirty = false;
    const data = await api("/api/load", {
      source: $("source").value,
      export: $("export-select").value,
      args: parseArgs($("args-input").value),
    });
    if (data.error && data.state) { render(data.state); return; }
    if (!data.error) {
      state.maxStep = 0;
      state.memoryStart = 0;
      render(data);
    }
  }

  function bind() {
    $("btn-load").onclick = load;
    $("btn-restart").onclick = () => { state.maxStep = 0; action("restart"); };
    $("btn-run").onclick = () => action("run");
    $("btn-step").onclick = () => action("step");
    $("btn-back").onclick = () => action("back");
    $("btn-finish").onclick = () => action("finish");

    $("source").addEventListener("input", () => { state.sourceDirty = true; });

    $("timeline").addEventListener("input", () => {
      const target = parseInt($("timeline").value, 10);
      $("timeline-meta").textContent = `rewinding to ${target}…`;
    });
    let rewindTimer = 0;
    $("timeline").addEventListener("change", () => {
      if (rewindTimer) clearTimeout(rewindTimer);
      rewindTimer = 0;
      action("rewind", parseInt($("timeline").value, 10));
    });
    $("timeline").addEventListener("input", () => {
      if (rewindTimer) clearTimeout(rewindTimer);
      rewindTimer = setTimeout(() => {
        rewindTimer = 0;
        action("rewind", parseInt($("timeline").value, 10));
      }, 160);
    });

    document.querySelectorAll(".tab").forEach((tab) => {
      tab.onclick = () => {
        state.tab = tab.dataset.tab;
        document.querySelectorAll(".tab").forEach((t) => t.classList.toggle("active", t === tab));
        document.querySelectorAll(".tab-body").forEach((b) => b.classList.remove("active"));
        const body = $(`tab-${state.tab}`);
        if (body) body.classList.add("active");
        if (state.value) renderTab(state.value);
      };
    });

    $("disasm").addEventListener("click", async (ev) => {
      const row = ev.target.closest(".disasm-row");
      if (!row) return;
      const func = parseInt(row.dataset.func, 10);
      const offset = parseInt(row.dataset.offset, 10);
      const stateData = await api("/api/breakpoints", { action: "toggle_instruction", func, offset });
      if (stateData && !stateData.error) render(stateData);
    });

    $("bp-host").addEventListener("change", async () => {
      const data = await api("/api/breakpoints", { action: $("bp-host").checked ? "host_on" : "host_off" });
      if (data && !data.error) render(data);
    });
    $("btn-bp-func").onclick = async () => {
      const data = await api("/api/breakpoints", { action: "add_function" });
      if (data && !data.error) render(data);
    };
    $("btn-bp-mem").onclick = async () => {
      const v = parseInt($("bp-mem-input").value, 10);
      if (!Number.isFinite(v)) { showError("memory breakpoint needs a number"); return; }
      const data = await api("/api/breakpoints", { action: "add_memory", address: v });
      if (data && !data.error) render(data);
    };
    $("btn-bp-clear").onclick = async () => {
      const data = await api("/api/breakpoints", { action: "clear" });
      if (data && !data.error) render(data);
    };
    $("bp-list").addEventListener("click", async (ev) => {
      const li = ev.target.closest("li[data-index]");
      if (!li) return;
      const idx = parseInt(li.dataset.index, 10);
      const data = await api("/api/breakpoints", { action: "remove", index: idx });
      if (data && !data.error) render(data);
    });

    $("btn-spec").onclick = async () => {
      $("spec-summary").textContent = "running…";
      $("spec-output").innerHTML = "";
      const data = await api("/api/spec", {});
      if (!data || data.error) { $("spec-summary").textContent = data ? data.error : "failed"; return; }
      $("spec-summary").textContent = `${data.passed}/${data.total} assertions passed`;
      $("spec-output").innerHTML = (data.reports || []).map((r) => {
        const cls = r.failed ? "fail" : "ok";
        const detail = r.failed
          ? (r.failures || []).map((f) => `<div>line ${f.line}: ${esc(f.message)}</div>`).join("")
          : "";
        return `<div class="spec-row ${cls}"><span>${esc(r.file)}</span><span class="count">${r.passed}/${r.total}</span></div>${detail}`;
      }).join("");
    };

    $("btn-diff").onclick = async () => {
      $("diff-output").textContent = "comparing…";
      const a = parseArgs($("diff-a").value);
      const b = parseArgs($("diff-b").value);
      const data = await api("/api/diff", { export: state.value ? state.value.export : "", args_a: a, args_b: b });
      if (!data || data.error) { $("diff-output").textContent = data ? data.error : "failed"; return; }
      if (!data.diverged) {
        $("diff-output").innerHTML = `<span>No divergence. ${data.left_steps} vs ${data.right_steps} steps.</span>`;
      } else {
        $("diff-output").innerHTML =
          `<div class="diverged">first divergence at step ${data.step}</div>
           <div>${esc(data.reason)}</div>
           <div>${esc(data.left_op || "—")} vs ${esc(data.right_op || "—")}</div>
           <div>${data.left_steps} vs ${data.right_steps} steps</div>`;
      }
    };

    $("btn-stream").onclick = streamTrace;
  }

  function streamTrace() {
    if (!state.value || !state.value.loaded) { showError("load a module first"); return; }
    $("message").textContent = "streaming via SSE…";
    const url = `/api/stream?token=${encodeURIComponent(token)}`;
    const es = new EventSource(url);
    let count = 0;
    const box = $("trace-list");
    box.innerHTML = "";
    es.addEventListener("step", (ev) => {
      try {
        const e = JSON.parse(ev.data);
        count += 1;
        const row = document.createElement("div");
        row.className = "trace-row";
        row.innerHTML = `<span class="step">${e.step}</span><span>func@${e.func}</span><span>${e.offset}</span><span class="op">${esc(e.op)}</span><span>${e.stack}</span><span>${e.depth}</span>`;
        box.appendChild(row);
        if (box.children.length > 80) box.removeChild(box.firstChild);
        box.scrollTop = box.scrollHeight;
      } catch { /* ignore malformed event */ }
    });
    es.addEventListener("done", (ev) => {
      try {
        const d = JSON.parse(ev.data);
        $("message").textContent = `SSE stream: ${d.steps} steps, trace ${d.trace_hash}`;
      } catch { /* ignore */ }
      es.close();
      fetchState();
    });
    es.addEventListener("error", () => {
      $("message").textContent = `SSE stream ended after ${count} events`;
      es.close();
    });
  }

  async function init() {
    bind();
    const s = await api(`/api/state`);
    if (s && (s.error ? s.state : s)) render(s.error ? s.state : s);
    else if (s && s.error === undefined) render(s);
    const first = state.value;
    if (first) {
      state.maxStep = Math.max(0, first.step || 0);
      state.memoryStart = 0;
    }
  }

  window.addEventListener("DOMContentLoaded", init);
})();
