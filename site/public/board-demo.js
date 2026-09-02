import { parseCapture } from "./capture.js";

(() => {
  const root = document.getElementById("tsk-demo");
  const frame = document.getElementById("board-demo");
  if (!root || !frame) return;

  const GLYPH = {
    ready: "○",
    started: "▸",
    blocked: "■",
    review: "▲",
    done: "✓",
  };

  const TABS = ["desk", "projects", "threads"];
  const NOW = Date.now();
  const MIN = 60 * 1000;
  const HOUR = 60 * MIN;
  const DAY = 24 * HOUR;

  const seed = () => {
    let n = 12;
    const task = (partial) => ({
      id: `t${n}`,
      number: n++,
      notes: "",
      thread: null,
      project: null,
      createdAt: NOW - 3 * MIN,
      updatedAt: NOW - 3 * MIN,
      ...partial,
    });
    return [
      task({
        title: "Smoke-test worktree dispatch",
        status: "started",
        project: "tsk",
        thread: "dispatch",
        notes:
          "Drive the real herdr path, read the pane, and fix anything that only fails live.",
        createdAt: NOW - 3 * MIN,
        updatedAt: NOW - 3 * MIN,
      }),
      task({
        title: "Edit target binding pin",
        status: "started",
        project: "herdr",
        notes: "Keep the save pin on the row the current lens still paints.",
        createdAt: NOW - 12 * MIN,
        updatedAt: NOW - 12 * MIN,
      }),
      task({
        title: "Global backlog note",
        status: "ready",
        notes: "Inbox capture. No project yet.",
        createdAt: NOW - 2 * DAY,
        updatedAt: NOW - 2 * DAY,
      }),
      task({
        title: "Waiting on the herdr pane label",
        status: "blocked",
        notes: "Desk-scoped. Unblocks to ready, not started.",
        createdAt: NOW - 9 * HOUR,
        updatedAt: NOW - 50 * MIN,
      }),
      task({
        title: "Check the landing copy once more",
        status: "review",
        notes: "Global review sits on desk ON DECK with blocked and ready.",
        createdAt: NOW - 5 * HOUR,
        updatedAt: NOW - 20 * MIN,
      }),
      task({
        title: "Document the verb modifier flip",
        status: "blocked",
        project: "tsk",
        thread: "docs",
        notes: "Waiting on the settings.json copy before the keys page can land.",
        createdAt: NOW - 6 * HOUR,
        updatedAt: NOW - 40 * MIN,
      }),
      task({
        title: "Review capture token edge cases",
        status: "review",
        project: "tsk",
        thread: "dispatch",
        notes: "Bare !p desk, !p /path verbatim, !t normalize to 32 chars.",
        createdAt: NOW - 1 * DAY,
        updatedAt: NOW - 90 * MIN,
      }),
      task({
        title: "Pane label matches BOARD_PANE_LABEL",
        status: "ready",
        project: "herdr",
        thread: "host",
        createdAt: NOW - 8 * HOUR,
        updatedAt: NOW - 8 * HOUR,
      }),
      task({
        title: "Ship the queue board milestone",
        status: "done",
        project: "tsk",
        createdAt: NOW - 2 * DAY,
        updatedAt: NOW - 5 * HOUR,
      }),
      task({
        title: "Retire classic board chrome",
        status: "done",
        createdAt: NOW - 2 * DAY,
        updatedAt: NOW - 6 * HOUR,
      }),
    ];
  };

  const state = {
    tasks: seed(),
    tab: "desk",
    focusProject: null,
    collapsed: new Set(),
    selectedId: "t1",
    peekId: null,
    flashId: null,
    copyNotice: "",
    drawer: false,
    overlay: null,
    draft: "",
    paletteQ: "",
    paletteI: 0,
    pickerI: 0,
    editField: null,
    editDraft: "",
    undo: null,
    refuse: "",
    nextId: 20,
    nextNumber: 22,
  };

  const esc = (s) =>
    String(s).replace(/[&<>"']/g, (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c],
    );

  const age = (ts) => {
    const secs = Math.max(0, Math.floor((Date.now() - ts) / 1000));
    if (secs < 60) return `${secs}s`;
    if (secs < 3600) return `${Math.floor(secs / 60)}m`;
    if (secs < 86400) return `${Math.floor(secs / 3600)}h`;
    return `${Math.floor(secs / 86400)}d`;
  };

  const projectName = (task) => task.project || "desk";
  const byUpdated = (a, b) => b.updatedAt - a.updatedAt;
  const taskById = (id) => state.tasks.find((t) => t.id === id);

  function knownProjects() {
    const set = new Set();
    for (const t of state.tasks) if (t.project) set.add(t.project);
    return ["desk", ...[...set].sort()];
  }

  function selectedTask() {
    return taskById(state.selectedId) || null;
  }

  function fallbackProject() {
    if (state.focusProject) return state.focusProject;
    return null;
  }

  function visibleTasks() {
    const open = state.tasks.filter((t) => t.status !== "done");
    const done = state.tasks.filter((t) => t.status === "done").sort(byUpdated);
    if (state.focusProject) {
      const inP = (t) =>
        state.focusProject === "desk" ? !t.project : t.project === state.focusProject;
      return {
        started: open.filter((t) => inP(t) && t.status === "started").sort(byUpdated),
        review: open.filter((t) => inP(t) && t.status === "review").sort(byUpdated),
        blocked: open.filter((t) => inP(t) && t.status === "blocked").sort(byUpdated),
        ready: open.filter((t) => inP(t) && t.status === "ready").sort(byUpdated),
        done: done.filter(inP),
      };
    }
    if (state.tab === "desk") {
      return {
        started: open.filter((t) => t.status === "started").sort(byUpdated),
        desk: open
          .filter(
            (t) =>
              !t.project &&
              (t.status === "ready" || t.status === "blocked" || t.status === "review"),
          )
          .sort(byUpdated),
        done,
      };
    }
    return { open, done };
  }

  function buildRows() {
    const rows = [];
    const pushHeader = (kind, label, count, extra = {}) => {
      rows.push({ kind, label, count, selectable: false, ...extra });
    };
    const pushTask = (task, indent = 0) => {
      rows.push({ kind: "task", task, indent, selectable: true, id: task.id });
    };

    if (state.tab === "desk" && !state.focusProject) {
      const v = visibleTasks();
      pushHeader("section", "IN MOTION", v.started.length);
      v.started.forEach((t) => pushTask(t));
      pushHeader("section", "desk", v.desk.length);
      v.desk.forEach((t) => pushTask(t));
      if (state.drawer) {
        pushHeader("section", "DONE", v.done.length);
        v.done.forEach((t) => pushTask(t));
      }
      return rows;
    }

    if (state.focusProject) {
      const v = visibleTasks();
      pushHeader("section", "IN MOTION", v.started.length);
      v.started.forEach((t) => pushTask(t));
      pushHeader("section", "ON DECK", v.review.length + v.blocked.length + v.ready.length);
      if (v.review.length) {
        pushHeader("sub", "review", v.review.length);
        v.review.forEach((t) => pushTask(t, 1));
      }
      if (v.blocked.length) {
        pushHeader("sub", "blocked", v.blocked.length);
        v.blocked.forEach((t) => pushTask(t, 1));
      }
      v.ready.forEach((t) => pushTask(t));
      if (state.drawer) {
        pushHeader("section", "DONE", v.done.length);
        v.done.forEach((t) => pushTask(t));
      }
      return rows;
    }

    if (state.tab === "projects") {
      const names = [...new Set(state.tasks.map((t) => t.project || "desk"))].sort((a, b) => {
        if (a === "desk") return 1;
        if (b === "desk") return -1;
        return a.localeCompare(b);
      });
      for (const name of names) {
        const key = `p:${name}`;
        const collapsed = state.collapsed.has(key);
        const group = state.tasks.filter((t) => (t.project || "desk") === name && t.status !== "done");
        const started = group.filter((t) => t.status === "started").sort(byUpdated);
        const review = group.filter((t) => t.status === "review").sort(byUpdated);
        const blocked = group.filter((t) => t.status === "blocked").sort(byUpdated);
        const ready = group.filter((t) => t.status === "ready").sort(byUpdated);
        pushHeader("group", name, group.length, { collapseKey: key, collapsed, project: name });
        if (collapsed) continue;
        if (started.length) {
          pushHeader("sub", "in motion", started.length);
          started.forEach((t) => pushTask(t, 1));
        }
        if (review.length) {
          pushHeader("sub", "review", review.length);
          review.forEach((t) => pushTask(t, 1));
        }
        if (blocked.length) {
          pushHeader("sub", "blocked", blocked.length);
          blocked.forEach((t) => pushTask(t, 1));
        }
        if (ready.length) {
          pushHeader("sub", "open", ready.length);
          ready.forEach((t) => pushTask(t, 1));
        }
      }
      if (state.drawer) {
        const done = state.tasks.filter((t) => t.status === "done").sort(byUpdated);
        pushHeader("section", "DONE", done.length);
        done.forEach((t) => pushTask(t));
      }
      return rows;
    }

    const threads = new Map();
    for (const t of state.tasks.filter((x) => x.status !== "done" && x.thread)) {
      if (!threads.has(t.thread)) threads.set(t.thread, []);
      threads.get(t.thread).push(t);
    }
    const names = [...threads.keys()].sort();
    for (const thread of names) {
      const tKey = `t:${thread}`;
      const tCollapsed = state.collapsed.has(tKey);
      const members = threads.get(thread);
      pushHeader("group", `#${thread}`, members.length, { collapseKey: tKey, collapsed: tCollapsed, thread });
      if (tCollapsed) continue;
      const byProj = new Map();
      for (const t of members) {
        const p = t.project || "desk";
        if (!byProj.has(p)) byProj.set(p, []);
        byProj.get(p).push(t);
      }
      for (const [proj, list] of [...byProj.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
        const pKey = `t:${thread}:${proj}`;
        const pCollapsed = state.collapsed.has(pKey);
        pushHeader("group", proj, list.length, { collapseKey: pKey, collapsed: pCollapsed, indent: 1 });
        if (pCollapsed) continue;
        const order = ["started", "review", "blocked", "ready"];
        for (const st of order) {
          const slice = list.filter((t) => t.status === st).sort(byUpdated);
          slice.forEach((t) => pushTask(t, 2));
        }
      }
    }
    const unthreaded = state.tasks.filter((t) => t.status !== "done" && !t.thread);
    if (unthreaded.length) {
      const key = "t:unthreaded";
      const collapsed = state.collapsed.has(key);
      pushHeader("group", "unthreaded", unthreaded.length, { collapseKey: key, collapsed });
      if (!collapsed) unthreaded.sort(byUpdated).forEach((t) => pushTask(t, 1));
    }
    if (state.drawer) {
      const done = state.tasks.filter((t) => t.status === "done").sort(byUpdated);
      pushHeader("section", "DONE", done.length);
      done.forEach((t) => pushTask(t));
    }
    return rows;
  }

  function selectableIds(rows) {
    return rows.filter((r) => r.selectable).map((r) => r.id);
  }

  function ensureSelection(rows) {
    const ids = selectableIds(rows);
    if (!ids.length) {
      state.selectedId = null;
      return;
    }
    if (!ids.includes(state.selectedId)) state.selectedId = ids[0];
  }

  function metaFor(task) {
    const bits = [];
    if (state.tab === "desk" && task.project) bits.push(task.project);
    bits.push(age(task.updatedAt));
    return bits.join(" · ");
  }

  function showCopyNotice(message) {
    state.copyNotice = message;
    setTimeout(() => {
      if (state.copyNotice === message) {
        state.copyNotice = "";
        render();
      }
    }, 2000);
    render();
  }

  function copyTaskIdentifier(task) {
    const identifier = `T${task.number}`;
    if (!navigator.clipboard?.writeText) {
      showCopyNotice("copy unavailable");
      return;
    }
    navigator.clipboard
      .writeText(identifier)
      .then(() => showCopyNotice(`copied ${identifier}`))
      .catch(() => showCopyNotice("copy failed"));
  }

  function verbItems(task) {
    if (!task) {
      return [
        { id: "open", label: "enter open" },
        { id: "capture", label: "+ capture" },
        { id: "help", label: "? help" },
        { id: "palette", label: ": palette" },
      ];
    }
    const items = [{ id: "open", label: "enter open" }];
    if (task.status === "ready") items.push({ id: "start", label: "s start" });
    if (task.status === "done") {
      items.push({ id: "reopen", label: "s reopen" });
      items.push({ id: "reopen", label: "o reopen" });
    } else {
      items.push({ id: "done", label: "d done" });
      items.push({ id: "block", label: task.status === "blocked" ? "b unblock" : "b block" });
    }
    items.push({ id: "palette", label: ": palette" }, { id: "help", label: "? help" }, { id: "capture", label: "+ capture" });
    return items;
  }

  function runVerb(id) {
    if (id === "open" && state.selectedId) state.overlay = "page";
    if (id === "capture") openQuickAdd();
    if (id === "help") state.overlay = "help";
    if (id === "palette") {
      state.overlay = "palette";
      state.paletteQ = "";
      state.paletteI = 0;
    }
    if (id === "start" || id === "reopen") primaryVerb();
    if (id === "done") setStatus("done");
    if (id === "block") toggleBlock();
  }

  function peekLines(task) {
    const notes = (task.notes || "").trim();
    if (!notes) return ["no notes yet"];
    return notes.split(/\n/).slice(0, 5);
  }

  function rule(label, count) {
    const pad = Math.max(4, 52 - label.length);
    return `${"─".repeat(pad)}${count}`;
  }

  function paletteCommands() {
    const q = state.paletteQ.trim().toLowerCase();
    const all = [
      { id: "ready", label: "set status: ready", run: () => setStatus("ready") },
      { id: "started", label: "set status: started", run: () => setStatus("started") },
      { id: "blocked", label: "set status: blocked", run: () => setStatus("blocked") },
      { id: "review", label: "set status: review", run: () => setStatus("review") },
      { id: "done", label: "set status: done", run: () => setStatus("done") },
      { id: "desk", label: "go desk", run: () => goTab("desk") },
      { id: "projects", label: "go projects", run: () => goTab("projects") },
      { id: "threads", label: "go threads", run: () => goTab("threads") },
      { id: "capture", label: "capture", run: () => openQuickAdd() },
      { id: "help", label: "help", run: () => (state.overlay = "help") },
      { id: "reset", label: "reset demo", run: resetDemo },
    ];
    if (!state.focusProject && ["projects", "threads"].includes(state.tab)) {
      all.push({ id: "groups", label: "toggle groups", run: toggleAllGroups });
    }
    return all.filter((c) => !q || c.label.includes(q) || c.id.includes(q));
  }

  function setStatus(status) {
    const task = selectedTask();
    if (!task) return;
    task.status = status;
    task.updatedAt = Date.now();
    if (status !== "done") state.drawer = state.drawer;
    state.flashId = task.id;
    setTimeout(() => {
      if (state.flashId === task.id) {
        state.flashId = null;
        render();
      }
    }, 420);
  }

  function goTab(tab) {
    state.tab = tab;
    state.focusProject = null;
    state.peekId = null;
    state.overlay = null;
  }

  function toggleAllGroups() {
    if (state.focusProject || !["projects", "threads"].includes(state.tab)) return;
    const groups = buildRows().filter((row) => row.kind === "group" && !row.indent);
    const collapse = groups.some((row) => !state.collapsed.has(row.collapseKey));
    for (const row of groups) {
      if (collapse) state.collapsed.add(row.collapseKey);
      else state.collapsed.delete(row.collapseKey);
    }
  }

  function resetDemo() {
    state.tasks = seed();
    state.tab = "desk";
    state.focusProject = null;
    state.collapsed = new Set();
    state.selectedId = "t1";
    state.peekId = null;
    state.drawer = false;
    state.overlay = null;
    state.draft = "";
    state.refuse = "";
    state.copyNotice = "";
    state.undo = null;
  }

  function openQuickAdd() {
    state.overlay = "quick";
    state.draft = "";
    state.refuse = "";
  }

  function saveDraft(stay) {
    const parsed = parseCapture(state.draft, fallbackProject());
    if (!parsed.title) {
      state.refuse = "title needed";
      return false;
    }
    const id = `n${state.nextId++}`;
    const now = Date.now();
    const task = {
      id,
      number: state.nextNumber++,
      title: parsed.title,
      notes: "",
      status: "ready",
      project: parsed.project,
      thread: parsed.thread === undefined ? null : parsed.thread,
      createdAt: now,
      updatedAt: now,
    };
    state.tasks.unshift(task);
    state.selectedId = id;
    state.flashId = id;
    state.refuse = "";
    state.draft = "";
    if (!stay) state.overlay = null;
    setTimeout(() => {
      if (state.flashId === id) {
        state.flashId = null;
        render();
      }
    }, 420);
    return true;
  }

  function deleteSelected() {
    const task = selectedTask();
    if (!task) return;
    state.undo = { task: { ...task }, index: state.tasks.indexOf(task) };
    state.tasks = state.tasks.filter((t) => t.id !== task.id);
    state.peekId = null;
  }

  function undoDelete() {
    if (!state.undo) return;
    const { task, index } = state.undo;
    state.tasks.splice(Math.min(index, state.tasks.length), 0, task);
    state.selectedId = task.id;
    state.undo = null;
  }

  function toggleBlock() {
    const task = selectedTask();
    if (!task || task.status === "done") return;
    setStatus(task.status === "blocked" ? "ready" : "blocked");
  }

  function primaryVerb() {
    const task = selectedTask();
    if (!task) return;
    if (task.status === "ready") setStatus("started");
    else if (task.status === "done") setStatus("ready");
  }

  function renderHelp() {
    return `
      <div class="tsk-overlay tsk-help">
        <div class="tsk-help-title">keys</div>
        <div class="tsk-help-body">
          <div>esc close | click a verb to run it</div>
          <div>j/k · ↑/↓ move | s primary</div>
          <div>d done | o reopen | b block</div>
          <div>enter open | →/← peek | + capture</div>
          <div>e title | n notes | x delete | u undo</div>
          <div>z drawer | : palette | ? help</div>
          <div>P project | 1 2 3 tabs | g groups (projects/threads)</div>
          <div class="dim">app needs ctrl on verbs · demo also accepts bare keys</div>
          <div class="dim">any key to close</div>
        </div>
      </div>`;
  }

  function renderPalette() {
    const cmds = paletteCommands();
    if (state.paletteI >= cmds.length) state.paletteI = Math.max(0, cmds.length - 1);
    const list = cmds
      .map((c, i) => {
        const mark = i === state.paletteI ? "▸" : " ";
        const cls = i === state.paletteI ? "sel-text" : "";
        return `<div class="tsk-pal-row ${cls}" data-cmd="${esc(c.id)}">${mark} ${esc(c.label)}</div>`;
      })
      .join("");
    return `
      <div class="tsk-overlay tsk-palette">
        <div class="tsk-help-title">command</div>
        ${list || `<div class="dim">no matches</div>`}
      </div>
      <div class="tsk-input-row"><span class="tsk-prompt">:</span><span class="tsk-draft">${esc(state.paletteQ)}</span><span class="cursor">█</span></div>
      <div class="foot dim">enter run · esc close · type to filter</div>`;
  }

  function renderPicker() {
    const opts = knownProjects();
    if (state.pickerI >= opts.length) state.pickerI = Math.max(0, opts.length - 1);
    const list = opts
      .map((name, i) => {
        const mark = i === state.pickerI ? "▸" : " ";
        const cls = i === state.pickerI ? "sel-text" : "";
        const current = state.focusProject === name || (!state.focusProject && name === "desk" && state.tab === "desk");
        return `<div class="tsk-pal-row ${cls}" data-pick="${esc(name)}">${mark} ${esc(name)}${current ? "  ·" : ""}</div>`;
      })
      .join("");
    return `
      <div class="tsk-overlay tsk-palette">
        <div class="tsk-help-title">project</div>
        ${list}
      </div>
      <div class="foot dim">enter choose · esc close · j/k move</div>`;
  }

  function renderPage() {
    const task = selectedTask();
    if (!task) return `<div class="tsk-overlay"><div class="dim">no task</div></div>`;
    const editing = state.editField;
    const title =
      editing === "title"
        ? `<input class="tsk-field" id="tsk-edit" value="${esc(state.editDraft)}" />`
        : `<div class="tsk-page-title"><span class="tsk-task-id" data-copy-task="${esc(task.id)}" title="copy T${task.number}">T${task.number}</span> ${esc(task.title)}</div>`;
    const notes =
      editing === "notes"
        ? `<textarea class="tsk-field tsk-notes" id="tsk-edit">${esc(state.editDraft)}</textarea>`
        : `<div class="tsk-page-notes">${esc(task.notes || "no notes yet")}</div>`;
    return `
      <div class="tsk-overlay tsk-page">
        ${title}
        <div class="dim">${esc(task.status)} · ${esc(projectName(task))}${task.thread ? ` · #${esc(task.thread)}` : ""}</div>
        ${notes}
        <div class="foot dim">esc close · e title · n notes · d done · b block</div>
      </div>`;
  }

  function renderBoard(rows) {
    const tabs = TABS.map((tab) => {
      const on = !state.focusProject && state.tab === tab;
      return `<button type="button" class="tsk-tab ${on ? "is-on" : ""}" data-tab="${tab}">${tab}</button>`;
    }).join(`<span class="dim">  ·  </span>`);

    const chip = state.focusProject
      ? `<button type="button" class="tsk-chip" data-chip="1">P ▾ ${esc(state.focusProject)}</button>`
      : "";

    const body = rows
      .map((row) => {
        if (row.kind === "section" || row.kind === "sub") {
          const cls = row.kind === "sub" ? "tsk-sub" : "tsk-sec";
          return `<div class="${cls}"><span class="sec">${esc(row.label)}</span> <span class="rule">${esc(rule(row.label, row.count))}</span></div>`;
        }
        if (row.kind === "group") {
          const mark = row.collapsed ? "▸" : "▾";
          const pad = row.indent ? "  " : "";
          return `<button type="button" class="tsk-group" data-collapse="${esc(row.collapseKey)}" data-project="${esc(row.project || "")}">${pad}<span class="dim">${mark}</span> <span class="sec">${esc(row.label)}</span> <span class="count">${row.count}</span></button>`;
        }
        const task = row.task;
        const selected = task.id === state.selectedId;
        const flash = task.id === state.flashId;
        const glyph = GLYPH[task.status] || "○";
        const indent = "  ".repeat(row.indent || 0);
        const peek =
          state.peekId === task.id
            ? [
                ...peekLines(task).map((line) => `<div class="tsk-peek dim">${indent}    │ ${esc(line)}</div>`),
                `<div class="tsk-peek dim">${indent}    └</div>`,
              ].join("")
            : "";
        return `<button type="button" class="tsk-row ${selected ? "is-sel" : ""} ${flash ? "is-flash" : ""}" data-task="${task.id}">
          <span class="tsk-row-main">${indent}  <span class="${selected ? "sel" : "glyph"}">${glyph}</span> <span class="tsk-task-id ${selected ? "sel-text" : ""}" data-copy-task="${esc(task.id)}" title="copy T${task.number}">T${task.number}</span> <span class="${selected ? "sel-text" : ""}">${esc(task.title)}</span></span>
          <span class="meta">${esc(metaFor(task))}</span>
        </button>${peek}`;
      })
      .join("");

    const doneN = state.tasks.filter((t) => t.status === "done").length;
    const task = selectedTask();
    const footer =
      state.overlay === "quick"
        ? `<div class="tsk-input-row"><span class="tsk-prompt">+</span><input class="tsk-field" id="tsk-add" value="${esc(state.draft)}" placeholder="title  ·  !p project  ·  !t thread" autocomplete="off" /><span class="cursor">█</span></div>
           <div class="foot dim">${state.refuse ? esc(state.refuse) : "enter save · shift+enter stay · tab page · esc close"}</div>`
        : `<button type="button" class="tsk-done-count foot" data-drawer="1">${doneN} done</button>
           <div class="foot dim tsk-verbs">${verbItems(task)
             .map((v) => `<button type="button" class="tsk-verb" data-verb="${esc(v.id)}">${esc(v.label)}</button>`)
             .join("<span> · </span>")}</div>
           ${state.copyNotice ? `<div class="foot dim">${esc(state.copyNotice)}</div>` : ""}`;

    return `
      <div class="tsk-tabs">${state.focusProject ? chip : tabs}</div>
      <div class="tsk-list">${body || `<div class="dim">  nothing here</div>`}</div>
      <div class="tsk-foot">
        <div class="foot-rule">──────────────────────────────────────────────────────────────</div>
        ${footer}
      </div>`;
  }

  function render() {
    const rows = buildRows();
    ensureSelection(rows);
    let html = renderBoard(rows);
    if (state.overlay === "help") html += renderHelp();
    if (state.overlay === "palette") html += renderPalette();
    if (state.overlay === "picker") html += renderPicker();
    if (state.overlay === "page") html += renderPage();
    const keepKeys =
      document.activeElement === frame || frame.contains(document.activeElement);
    root.innerHTML = html;
    const add = document.getElementById("tsk-add");
    const edit = document.getElementById("tsk-edit");
    if (add) {
      add.focus();
      add.selectionStart = add.value.length;
      add.addEventListener("input", () => {
        state.draft = add.value;
        state.refuse = "";
      });
    } else if (edit) {
      edit.focus();
      edit.addEventListener("input", () => {
        state.editDraft = edit.value;
      });
    } else if (keepKeys) {
      frame.focus({ preventScroll: true });
    }
    frame.classList.toggle(
      "is-focused",
      document.activeElement === frame || frame.contains(document.activeElement),
    );
  }

  function move(delta) {
    const ids = selectableIds(buildRows());
    if (!ids.length) return;
    let i = ids.indexOf(state.selectedId);
    if (i < 0) i = 0;
    i = (i + delta + ids.length) % ids.length;
    state.selectedId = ids[i];
    state.peekId = state.peekId && state.peekId === state.selectedId ? state.peekId : null;
  }

  function commitEdit() {
    const task = selectedTask();
    if (!task || !state.editField) return;
    if (state.editField === "title") {
      const title = state.editDraft.trim();
      if (title) task.title = title;
    } else {
      task.notes = state.editDraft;
    }
    task.updatedAt = Date.now();
    state.editField = null;
    state.editDraft = "";
  }

  function onKey(e) {
    if (state.overlay === "quick") {
      if (e.key === "Escape") {
        e.preventDefault();
        state.overlay = null;
        state.refuse = "";
        frame.focus();
        render();
        return;
      }
      if (e.key === "Enter" && !e.ctrlKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        saveDraft(e.shiftKey);
        if (!e.shiftKey) frame.focus();
        render();
        return;
      }
      if (e.key === "Tab") {
        e.preventDefault();
        if (saveDraft(false)) {
          state.overlay = "page";
          state.editField = "notes";
          state.editDraft = "";
        }
        render();
      }
      return;
    }

    if (state.overlay === "page" && state.editField) {
      if (e.key === "Escape") {
        e.preventDefault();
        state.editField = null;
        frame.focus();
        render();
        return;
      }
      if (e.key === "Enter" && state.editField === "title") {
        e.preventDefault();
        commitEdit();
        render();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
        e.preventDefault();
        commitEdit();
        render();
      }
      return;
    }

    if (state.overlay === "help") {
      e.preventDefault();
      state.overlay = null;
      render();
      return;
    }

    if (state.overlay === "palette") {
      if (e.key === "Escape") {
        e.preventDefault();
        state.overlay = null;
        render();
        return;
      }
      if (e.key === "Enter") {
        e.preventDefault();
        const cmds = paletteCommands();
        const cmd = cmds[state.paletteI];
        state.overlay = null;
        if (cmd) cmd.run();
        render();
        return;
      }
      if (e.key === "ArrowDown" || e.key === "j") {
        e.preventDefault();
        state.paletteI += 1;
        render();
        return;
      }
      if (e.key === "ArrowUp" || e.key === "k") {
        e.preventDefault();
        state.paletteI = Math.max(0, state.paletteI - 1);
        render();
        return;
      }
      if (e.key === "Backspace") {
        e.preventDefault();
        state.paletteQ = state.paletteQ.slice(0, -1);
        state.paletteI = 0;
        render();
        return;
      }
      if (e.key.length === 1 && !e.altKey && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        state.paletteQ += e.key;
        state.paletteI = 0;
        render();
      }
      return;
    }

    if (state.overlay === "picker") {
      const opts = knownProjects();
      if (e.key === "Escape") {
        e.preventDefault();
        state.overlay = null;
        render();
        return;
      }
      if (e.key === "Enter") {
        e.preventDefault();
        const name = opts[state.pickerI];
        state.focusProject = name === "desk" ? null : name;
        if (name === "desk") state.tab = "desk";
        state.overlay = null;
        render();
        return;
      }
      if (e.key === "ArrowDown" || e.key === "j") {
        e.preventDefault();
        state.pickerI = Math.min(opts.length - 1, state.pickerI + 1);
        render();
        return;
      }
      if (e.key === "ArrowUp" || e.key === "k") {
        e.preventDefault();
        state.pickerI = Math.max(0, state.pickerI - 1);
        render();
      }
      return;
    }

    const alt = e.altKey || e.ctrlKey || e.metaKey;
    if (state.overlay === "page" && !e.altKey && !e.ctrlKey && ["j", "k", "ArrowDown", "ArrowUp", "1", "2", "3", "+", "z", ":"].includes(e.key)) {
      e.preventDefault();
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      if (state.overlay === "page") state.overlay = null;
      else if (state.peekId) state.peekId = null;
      else if (state.focusProject) state.focusProject = null;
      else frame.blur();
      render();
      return;
    }
    if (e.key === "?" ) {
      e.preventDefault();
      state.overlay = "help";
      render();
      return;
    }
    if (e.key === ":") {
      e.preventDefault();
      state.overlay = "palette";
      state.paletteQ = "";
      state.paletteI = 0;
      render();
      return;
    }
    if (e.key === "+") {
      e.preventDefault();
      openQuickAdd();
      render();
      return;
    }
    if (e.key === "1" || e.key === "2" || e.key === "3") {
      e.preventDefault();
      goTab(TABS[Number(e.key) - 1]);
      render();
      return;
    }
    if (e.key === "P") {
      e.preventDefault();
      state.overlay = "picker";
      state.pickerI = 0;
      render();
      return;
    }
    if (e.key === "z") {
      e.preventDefault();
      state.drawer = !state.drawer;
      render();
      return;
    }
    if (e.key === "g" && !e.altKey && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      toggleAllGroups();
      render();
      return;
    }
    if (e.key === "j" || e.key === "ArrowDown") {
      e.preventDefault();
      move(1);
      render();
      return;
    }
    if (e.key === "k" || e.key === "ArrowUp") {
      e.preventDefault();
      move(-1);
      render();
      return;
    }
    if (e.key === "ArrowRight") {
      e.preventDefault();
      state.peekId = state.selectedId;
      render();
      return;
    }
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      state.peekId = null;
      render();
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      if (state.selectedId) state.overlay = "page";
      render();
      return;
    }
    if (e.key === "s") {
      e.preventDefault();
      primaryVerb();
      render();
      return;
    }
    if (e.key === "d") {
      e.preventDefault();
      setStatus("done");
      render();
      return;
    }
    if (e.key === "o") {
      e.preventDefault();
      const task = selectedTask();
      if (task && task.status === "done") setStatus("ready");
      render();
      return;
    }
    if (e.key === "b") {
      e.preventDefault();
      toggleBlock();
      render();
      return;
    }
    if (e.key === "x") {
      e.preventDefault();
      deleteSelected();
      render();
      return;
    }
    if (e.key === "u") {
      e.preventDefault();
      undoDelete();
      render();
      return;
    }
    if (e.key === "e") {
      e.preventDefault();
      const task = selectedTask();
      if (task) {
        state.overlay = "page";
        state.editField = "title";
        state.editDraft = task.title;
      }
      render();
      return;
    }
    if (e.key === "n") {
      e.preventDefault();
      const task = selectedTask();
      if (task) {
        state.overlay = "page";
        state.editField = "notes";
        state.editDraft = task.notes || "";
      }
      render();
      return;
    }
    if (e.key === "q" && alt) {
      e.preventDefault();
      frame.blur();
      render();
    }
  }

  let lastClick = { id: null, at: 0 };

  root.addEventListener("click", (e) => {
    const copy = e.target.closest("[data-copy-task]");
    if (copy) {
      const task = state.tasks.find((item) => item.id === copy.getAttribute("data-copy-task"));
      if (task) copyTaskIdentifier(task);
      render();
      return;
    }
    const tab = e.target.closest("[data-tab]");
    if (tab) {
      goTab(tab.getAttribute("data-tab"));
      render();
      return;
    }
    const chip = e.target.closest("[data-chip]");
    if (chip) {
      state.overlay = "picker";
      state.pickerI = 0;
      render();
      return;
    }
    const group = e.target.closest("[data-collapse]");
    if (group) {
      const now = Date.now();
      const key = group.getAttribute("data-collapse");
      const project = group.getAttribute("data-project");
      if (lastClick.id === key && now - lastClick.at < 350 && project) {
        state.focusProject = project === "desk" ? null : project;
        if (project === "desk") state.tab = "desk";
      } else if (state.collapsed.has(key)) state.collapsed.delete(key);
      else state.collapsed.add(key);
      lastClick = { id: key, at: now };
      render();
      return;
    }
    const row = e.target.closest("[data-task]");
    if (row) {
      const id = row.getAttribute("data-task");
      const now = Date.now();
      if (lastClick.id === id && now - lastClick.at < 350) {
        state.selectedId = id;
        state.overlay = "page";
      } else {
        state.selectedId = id;
        state.peekId = state.peekId === id ? null : id;
      }
      lastClick = { id, at: now };
      render();
      return;
    }
    const drawer = e.target.closest("[data-drawer]");
    if (drawer) {
      state.drawer = !state.drawer;
      render();
      return;
    }
    const cmd = e.target.closest("[data-cmd]");
    if (cmd && state.overlay === "palette") {
      const hit = paletteCommands().find((c) => c.id === cmd.getAttribute("data-cmd"));
      state.overlay = null;
      if (hit) hit.run();
      render();
      return;
    }
    const verb = e.target.closest("[data-verb]");
    if (verb) {
      runVerb(verb.getAttribute("data-verb"));
      render();
      return;
    }
    const pick = e.target.closest("[data-pick]");
    if (pick) {
      const name = pick.getAttribute("data-pick");
      state.focusProject = name === "desk" ? null : name;
      if (name === "desk") state.tab = "desk";
      state.overlay = null;
      render();
    }
  });

  frame.addEventListener("keydown", onKey);
  frame.addEventListener("focusin", () => frame.classList.add("is-focused"));
  frame.addEventListener("focusout", (e) => {
    if (!frame.contains(e.relatedTarget)) frame.classList.remove("is-focused");
  });

  render();
})();
