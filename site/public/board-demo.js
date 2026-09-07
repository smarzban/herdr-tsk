import { parseCapture } from "./capture.js";

(() => {
  const root = document.getElementById("tsk-demo");
  const frame = document.getElementById("board-demo");
  if (!root || !frame) return;

  const WIDE_SPLIT_MIN_COLUMNS = 110;

  const GLYPH = {
    ready: "○",
    started: "▸",
    blocked: "■",
    review: "▲",
    done: "✓",
  };

  const TABS = [
    ["desk", "desk"],
    ["project", "selected project"],
    ["projects", "projects"],
  ];
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
      archived: false,
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
        status: "ready",
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
        notes: "Global review sits on desk NEEDS YOU with blocked.",
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
      task({
        title: "File the old vendored spike away",
        status: "ready",
        archived: true,
        notes: "Archived: kept, off the radar. Find it in the drawer's archived group.",
        createdAt: NOW - 3 * DAY,
        updatedAt: NOW - 1 * DAY,
      }),
    ];
  };

  const state = {
    tasks: seed(),
    tab: "desk",
    // The focused scope is transient, while this is the project selected by tab 2.
    selectedProject: "tsk",
    focusProject: null,
    projectQuery: "",
    collapsed: new Set(),
    selectedId: "t1",
    peekId: null,
    flashId: null,
    copyNotice: "",
    drawer: false,
    archivedOpen: false,
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
    // Wide stage slider: board · split · rail · page. Focus is the stage.
    stage: "board",
    stageOrigin: null,
  };

  const TASK_STAGES = ["rail", "page"];
  function taskFocus() {
    return TASK_STAGES.includes(state.stage);
  }
  function stageRight() {
    if (!state.selectedId) return;
    if (state.stage === "board") state.stage = "split";
    else if (state.stage === "split") state.stage = "rail";
    else if (state.stage === "rail") {
      state.stageOrigin = "rail";
      state.stage = "page";
    }
  }
  function stageLeft() {
    if (state.stage === "split") state.stage = "board";
    else if (state.stage === "rail") state.stage = "split";
    else if (state.stage === "page") {
      state.stageOrigin = null;
      state.stage = "rail";
    }
  }
  function openFullPage() {
    if (!state.selectedId) return;
    if (state.stage !== "page") state.stageOrigin = state.stage;
    state.stage = "page";
  }
  function leaveTaskPage() {
    if (state.stage === "page") {
      state.stage = state.stageOrigin || "board";
      state.stageOrigin = null;
    } else if (state.stage === "rail") state.stage = "split";
  }
  function enterTaskStage() {
    if (state.stage === "board") {
      state.stageOrigin = "board";
      state.stage = "page";
    } else if (state.stage === "split") state.stage = "rail";
  }

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

  function projectNames() {
    const names = new Set(
      state.tasks.filter((t) => t.project && !t.archived).map((t) => t.project),
    );
    return [...names].sort((a, b) => {
      if (a === "tsk") return -1;
      if (b === "tsk") return 1;
      return a.localeCompare(b);
    });
  }

  function matchingProjectNames() {
    const query = state.projectQuery.trim().toLowerCase();
    return projectNames().filter((name) => !query || name.toLowerCase().includes(query));
  }

  function selectedTask() {
    return taskById(state.selectedId) || null;
  }

  function terminalColumns() {
    const fontSize = Number.parseFloat(getComputedStyle(root).fontSize) || 14;
    return Math.floor(root.getBoundingClientRect().width / (fontSize * 0.6));
  }

  function isWideSplit() {
    return terminalColumns() >= WIDE_SPLIT_MIN_COLUMNS;
  }

  function fallbackProject() {
    if (state.focusProject) return state.focusProject;
    return null;
  }

  function archivedInScope() {
    const archived = state.tasks.filter((t) => t.archived).sort(byUpdated);
    if (state.focusProject) {
      const inP = (t) =>
        state.focusProject === "desk" ? !t.project : t.project === state.focusProject;
      return archived.filter(inP);
    }
    return archived;
  }

  function visibleTasks() {
    // Hidden (archived) tasks leave every working view.
    const open = state.tasks.filter((t) => t.status !== "done" && !t.archived);
    const done = state.tasks.filter((t) => t.status === "done" && !t.archived).sort(byUpdated);
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
        need: open
          .filter((t) => t.status === "blocked" || t.status === "review")
          .sort(byUpdated),
        desk: open
          .filter((t) => !t.project && t.status === "ready")
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
      if (v.need.length) {
        pushHeader("section", "NEEDS YOU", v.need.length);
        v.need.forEach((t) => pushTask(t));
      }
      pushHeader("section", "IN MOTION", v.started.length);
      v.started.forEach((t) => pushTask(t));
      pushHeader("section", "desk", v.desk.length);
      v.desk.forEach((t) => pushTask(t));
      if (state.drawer) {
        pushHeader("section", "DONE", v.done.length);
        v.done.forEach((t) => pushTask(t));
        const archived = archivedInScope();
        if (archived.length) {
          rows.push({ kind: "archived", label: "archived", count: archived.length, selectable: true, id: "nav:archived" });
          if (state.archivedOpen) archived.forEach((t) => rows.push({ kind: "task", task: t, indent: 0, selectable: true, id: t.id, dim: true }));
        }
      }
      return rows;
    }

    if (state.focusProject) {
      const v = visibleTasks();
      const need = [...v.review, ...v.blocked].sort(byUpdated);
      if (need.length) {
        pushHeader("section", "NEEDS YOU", need.length);
        need.forEach((t) => pushTask(t));
      }
      pushHeader("section", "IN MOTION", v.started.length);
      v.started.forEach((t) => pushTask(t));
      pushHeader("section", "ON DECK", v.ready.length);
      v.ready.forEach((t) => pushTask(t));
      if (state.drawer) {
        pushHeader("section", "DONE", v.done.length);
        v.done.forEach((t) => pushTask(t));
        const archived = archivedInScope();
        if (archived.length) {
          rows.push({ kind: "archived", label: "archived", count: archived.length, selectable: true, id: "nav:archived" });
          if (state.archivedOpen) archived.forEach((t) => rows.push({ kind: "task", task: t, indent: 0, selectable: true, id: t.id, dim: true }));
        }
      }
      return rows;
    }

    if (state.tab === "projects") {
      for (const name of matchingProjectNames()) {
        const group = state.tasks.filter((t) => (t.project || "desk") === name && t.status !== "done" && !t.archived);
        const started = group.filter((t) => t.status === "started").sort(byUpdated);
        const review = group.filter((t) => t.status === "review").sort(byUpdated);
        const blocked = group.filter((t) => t.status === "blocked").sort(byUpdated);
        const ready = group.filter((t) => t.status === "ready").sort(byUpdated);
        rows.push({
          kind: "project",
          label: name,
          project: name,
          needs: review.length + blocked.length,
          motion: started.length,
          ready: ready.length,
          selectable: true,
          id: `project:${name}`,
        });
        // Project index rows are navigation, not collapsible task groups.
      }
      if (state.drawer) {
        const done = state.tasks.filter((t) => t.status === "done" && !t.archived).sort(byUpdated);
        pushHeader("section", "DONE", done.length);
        done.forEach((t) => pushTask(t));
        const archived = archivedInScope();
        if (archived.length) {
          rows.push({ kind: "archived", label: "archived", count: archived.length, selectable: true, id: "nav:archived" });
          if (state.archivedOpen) archived.forEach((t) => rows.push({ kind: "task", task: t, indent: 0, selectable: true, id: t.id, dim: true }));
        }
      }
      return rows;
    }

    if (state.drawer) {
      const done = state.tasks.filter((t) => t.status === "done" && !t.archived).sort(byUpdated);
      pushHeader("section", "DONE", done.length);
      done.forEach((t) => pushTask(t));
      const archived = archivedInScope();
      if (archived.length) {
        rows.push({ kind: "archived", label: "archived", count: archived.length, selectable: true, id: "nav:archived" });
        if (state.archivedOpen) archived.forEach((t) => rows.push({ kind: "task", task: t, indent: 0, selectable: true, id: t.id, dim: true }));
      }
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
    if (state.tab === "desk") bits.push(projectName(task));
    if (state.focusProject && task.thread) bits.push(`#${task.thread}`);
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
      // The bar is a prompt, not a keymap: open · status verbs · add · help.
      if (state.tab === "projects" && !state.focusProject) {
        return [
          { id: "open", label: "enter open" },
          { id: "search", label: "/ search" },
          { id: "help", label: "? help" },
        ];
      }
      return [
        { id: "capture", label: "+ add" },
        { id: "help", label: "? help" },
      ];
    }
    const items = [{ id: "open", label: "enter open" }];
    if (task.status === "ready") items.push({ id: "start", label: "s start" });
    if (task.status === "done") {
      items.push({ id: "reopen", label: "o reopen" });
    } else {
      items.push({ id: "done", label: "d done" });
      items.push({ id: "block", label: task.status === "blocked" ? "b unblock" : "b block" });
    }
    items.push({ id: "capture", label: "+ add" }, { id: "help", label: "? help" });
    return items;
  }

  function runVerb(id) {
    if (id === "search") state.overlay = "search";
    if (id === "open" && state.selectedId) openFullPage();
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
    if (id === "file") fileSelected();
  }

  // Word-wrap note lines to the board width so each visual line carries its own │ gutter,
  // as the app paints peek. At most five rows, like peekLines.
  function wrapPeek(lines, width) {
    const out = [];
    for (const line of lines) {
      const words = line.split(/\s+/).filter(Boolean);
      let current = "";
      for (const word of words) {
        if (!current) current = word;
        else if (current.length + 1 + word.length <= width) current += ` ${word}`;
        else {
          out.push(current);
          current = word;
        }
        if (out.length >= 5) return out.slice(0, 5);
      }
      if (current || !words.length) out.push(current);
      if (out.length >= 5) return out.slice(0, 5);
    }
    return out;
  }

  function peekLines(task) {
    const notes = (task.notes || "").trim();
    if (!notes) return ["no notes yet"];
    return notes.split(/\n/).slice(0, 5);
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
      { id: "project", label: "go selected project", run: () => goTab("project") },
      { id: "capture", label: "capture", run: () => openQuickAdd() },
      { id: "help", label: "help", run: () => (state.overlay = "help") },
      { id: "reset", label: "reset demo", run: resetDemo },
    ];
    if (!state.focusProject && state.tab === "projects") {
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
    state.focusProject = tab === "project" ? state.selectedProject : null;
    state.projectQuery = "";
    state.peekId = null;
    state.overlay = null;
    state.stage = "board";
    state.stageOrigin = null;
  }

  function openProject(name) {
    if (!name || name === "desk") {
      goTab("desk");
      return;
    }
    state.selectedProject = name;
    state.focusProject = name;
    state.tab = "project";
    state.projectQuery = "";
    state.peekId = null;
    state.overlay = null;
    state.stage = "board";
    state.stageOrigin = null;
  }

  function selectedRow() {
    return buildRows().find((row) => row.selectable && row.id === state.selectedId) || null;
  }

  function toggleAllGroups() {
    if (state.focusProject || state.tab !== "projects") return;
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
    state.selectedProject = "tsk";
    state.focusProject = null;
    state.projectQuery = "";
    state.collapsed = new Set();
    state.selectedId = "t1";
    state.peekId = null;
    state.drawer = false;
    state.overlay = null;
    state.draft = "";
    state.refuse = "";
    state.copyNotice = "";
    state.undo = null;
    state.stage = "board";
    state.stageOrigin = null;
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

  function fileSelected() {
    const task = selectedTask();
    if (!task) return;
    task.archived = !task.archived;
    state.peekId = null;
    // The file verb has no undo entry: ctrl+u never brings it back.
  }

  function deleteSelected() {
    const task = selectedTask();
    if (!task) return;
    state.undo = { task: { ...task }, index: state.tasks.indexOf(task) };
    state.tasks = state.tasks.filter((t) => t.id !== task.id);
    state.peekId = null;
  }

  function undoDelete() {
    if (selectedRow()?.kind !== "task" || !state.undo) return;
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
      <div class="tsk-box tsk-help" role="dialog" aria-label="help">
        <div class="tsk-box-top"><span class="tsk-box-title">help</span><button type="button" class="tsk-box-close" data-close="1" aria-label="close">[x]</button></div>
        <div class="tsk-box-body tsk-help-body">
          <div>board</div>
          <div>j/k · ↑/↓ move | enter open | →/← peek or slide</div>
          <div>s start / reopen | d done | o reopen | b block | r review</div>
          <div>e edit title | x delete | u undo | f archive | + add</div>
          <div>z or D drawer (app: d) | g archived group | p projects | 1 2 3 destinations</div>
          <div>/ search projects | : palette | ? help</div>
          <div class="dim">app needs ctrl on verbs · demo also accepts bare keys</div>
        </div>
        <div class="tsk-box-foot">esc close</div>
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
      <div class="tsk-box tsk-palette" role="dialog" aria-label="command">
        <div class="tsk-box-top"><span class="tsk-box-title">command</span><button type="button" class="tsk-box-close" data-close="1" aria-label="close">[x]</button></div>
        <div class="tsk-box-body">${list || `<div class="dim">  no matches</div>`}</div>
        <div class="tsk-box-foot">↑/↓ move · enter run · esc close · type to filter</div>
      </div>
      <div class="tsk-input-row"><span class="tsk-prompt">:</span><span class="tsk-draft">${esc(state.paletteQ)}</span><span class="cursor">█</span></div>`;
  }

  function renderPicker() {
    const opts = knownProjects();
    if (state.pickerI >= opts.length) state.pickerI = Math.max(0, opts.length - 1);
    const list = opts
      .map((name, i) => {
        const mark = i === state.pickerI ? "▸" : " ";
        const cls = i === state.pickerI ? "sel-text" : "";
        const current = name === "desk"
          ? !state.focusProject && state.tab === "desk"
          : state.selectedProject === name;
        return `<div class="tsk-pal-row ${cls}" data-pick="${esc(name)}">${mark} ${esc(name)}${current ? "  ·" : ""}</div>`;
      })
      .join("");
    return `
      <div class="tsk-box tsk-palette" role="dialog" aria-label="project">
        <div class="tsk-box-top"><span class="tsk-box-title">project</span><button type="button" class="tsk-box-close" data-close="1" aria-label="close">[x]</button></div>
        <div class="tsk-box-body">${list}</div>
        <div class="tsk-box-foot">↑/↓ move · enter choose · esc close</div>
      </div>`;
  }

  function runPageVerb(id) {
    enterTaskStage();
    const task = selectedTask();
    if (!task) return;
    if (id === "edit") {
      state.editField = "title";
      state.editDraft = task.title;
    }
    if (id === "notes") {
      state.editField = "notes";
      state.editDraft = task.notes || "";
    }
    if (id === "done") setStatus("done");
    if (id === "block") toggleBlock();
    if (id === "file") fileSelected();
  }

  // The wide task column: a header rule on the selector row (dim in split, bold when the task
  // owns focus) and the page body under it. Narrow: the same page fills the frame.
  function renderPage(embedded = false) {
    const task = selectedTask();
    const focused = taskFocus();
    if (!task) {
      if (embedded) {
        return `<div class="tsk-task-column tsk-surface" aria-label="task column"><div class="tsk-task-header dim"><span class="sec">no task</span></div><div class="tsk-task-rule" aria-hidden="true"></div><div class="tsk-task-surface"><div class="dim">  select a task to preview it here</div></div></div>`;
      }
      return `<div class="tsk-overlay"><div class="dim">no task</div><div class="dim">  select a task to preview it here</div></div>`;
    }
    const editing = state.editField;
    const stateSlot = editing ? `editing ${editing}` : `${task.status} · ${projectName(task)}`;
    const headTitle =
      editing === "title"
        ? `<input class="tsk-field" id="tsk-edit" value="${esc(state.editDraft)}" />`
        : esc(task.title);
    const glyph = GLYPH[task.status] || "○";
    const header = `<div class="tsk-task-header ${focused ? "is-bold" : "dim"}"><span class="glyph">${glyph}</span> <span class="tsk-task-id" data-copy-task="${esc(task.id)}" title="copy T${task.number}">T${task.number}</span> <span class="sec">${headTitle}</span><span class="tsk-state-slot">${esc(stateSlot)}</span></div><div class="tsk-task-rule" aria-hidden="true"></div>`;
    const notes =
      editing === "notes"
        ? `<textarea class="tsk-field tsk-notes" id="tsk-edit">${esc(state.editDraft)}</textarea>`
        : `<div class="tsk-page-notes">${esc(task.notes || "no notes yet")}</div>`;
    const meta = `<div class="dim">${task.thread ? `#${esc(task.thread)} · ` : ""}created ${esc(age(task.createdAt))} ago · updated ${esc(age(task.updatedAt))} ago</div>`;
    if (embedded) {
      return `<div class="tsk-task-column tsk-surface" aria-label="T${task.number} task column">${header}<div class="tsk-task-surface tsk-page">${notes}${meta}</div></div>`;
    }
    return `
      <div class="tsk-overlay tsk-page">
        ${header}
        ${notes}
        ${meta}
        <div class="foot dim tsk-verbs">${pageVerbBar()}</div>
      </div>`;
  }

  function stageHint() {
    if (!isWideSplit()) return "";
    if (state.editField) return "shift+enter save · esc cancel";
    if (state.stage === "board") return "→ pane · enter open";
    if (state.stage === "split") return "board ▸ task    → task · ← close · enter open";
    if (state.stage === "rail") return "board ◂ task    ← board · → full page";
    return "← rail · esc back";
  }

  function renderBoard(rows, rail = false, bare = false) {
    const tabs = TABS.map(([tab, label]) => {
      const on = state.tab === tab;
      const text = tab === "project" ? state.selectedProject : label;
      return `<button type="button" class="tsk-tab ${on ? "is-on" : ""}" data-tab="${tab}">${esc(text)}</button>`;
    }).join(`<span class="dim">  ·  </span>`);

    const chip = state.focusProject
      ? `<button type="button" class="tsk-chip" data-chip="1">P ▾ ${esc(state.focusProject)}</button>`
      : "";

    const body = rows
      .map((row) => {
        if (row.kind === "section" || row.kind === "sub") {
          const cls = row.kind === "sub" ? "tsk-sub" : "tsk-sec";
          return `<div class="${cls}"><span class="sec">${esc(row.label)}</span><span class="rule" aria-hidden="true"></span><span class="count">${row.count}</span></div>`;
        }
        if (row.kind === "project") {
          const selected = row.id === state.selectedId;
          return `<button type="button" class="tsk-group ${selected ? "sel-text" : ""}" data-project-row="${esc(row.project)}" data-nav-id="${esc(row.id)}"><span class="sec">${selected ? "▸" : " "} ${esc(row.label)}</span><span class="count">${row.needs} needs · ${row.motion} motion · ${row.ready} ready</span></button>`;
        }
        if (row.kind === "group") {
          const mark = row.collapsed ? "▸" : "▾";
          const pad = row.indent ? "  " : "";
          return `<button type="button" class="tsk-group" data-collapse="${esc(row.collapseKey)}" data-project="${esc(row.project || "")}">${pad}<span class="dim">${mark}</span> <span class="sec">${esc(row.label)}</span> <span class="count">${row.count}</span></button>`;
        }
        if (row.kind === "archived") {
          const mark = state.archivedOpen ? "▾" : "▸";
          const selected = row.id === state.selectedId;
          return `<button type="button" class="tsk-group" data-archived-header="1"><span class="dim">${mark}</span> <span class="sec">${selected ? "<strong>archived</strong>" : "archived"}</span><span class="rule" aria-hidden="true"></span><span class="count dim">${row.count}</span></button>`;
        }
        const task = row.task;
        if (rail && task.status === "done") return "";
        const selected = task.id === state.selectedId;
        const flash = task.id === state.flashId;
        const glyph = rail && selected ? "▹" : GLYPH[task.status] || "○";
        const indent = "  ".repeat(row.indent || 0);
        if (rail) {
          return `<button type="button" class="tsk-row tsk-rail-row" data-task="${task.id}">
          <span class="tsk-row-main">${indent}  <span class="glyph">${glyph}</span> <span class="tsk-task-id" data-copy-task="${esc(task.id)}" title="copy T${task.number}">T${task.number}</span> <span>${esc(task.title)}</span></span>
        </button>`;
        }
        const peek =
          state.peekId === task.id
            ? [
                ...wrapPeek(peekLines(task), Math.max(20, terminalColumns() - indent.length - 12)).map(
                  (line) => `<div class="tsk-peek dim">${indent}    │ ${esc(line)}</div>`,
                ),
                `<div class="tsk-peek dim">${indent}    └</div>`,
              ].join("")
            : "";
        const dimRow = row.dim ? "dim" : "";
        return `<button type="button" class="tsk-row ${dimRow} ${selected ? "is-sel" : ""} ${flash ? "is-flash" : ""}" data-task="${task.id}">
          <span class="tsk-row-main">${indent}  <span class="${selected ? "sel" : "glyph"}">${glyph}</span> <span class="tsk-task-id ${selected ? "sel-text" : ""}" data-copy-task="${esc(task.id)}" title="copy T${task.number}">T${task.number}</span> <span class="${selected ? "sel-text" : ""}">${esc(task.title)}</span></span>
          <span class="meta">${esc(metaFor(task))}</span>
        </button>${peek}`;
      })
      .join("");

    const column = `
      <div class="tsk-tabs">${state.focusProject ? chip : tabs}</div>
      <div class="tsk-list">${body || `<div class="dim">  nothing here</div>`}</div>`;
    // Wide stages paint one shared footer under both columns, so a column omits its own.
    return rail || bare ? column : column + renderFooter();
  }

  const PAGE_VERBS = [
    { id: "edit", label: "e edit" },
    { id: "done", label: "d done" },
    { id: "block", label: "b block" },
  ];

  function pageVerbBar() {
    return (
      PAGE_VERBS.map((v) => `<button type="button" class="tsk-verb" data-page-verb="${v.id}">${v.label}</button>`).join(
        "<span> · </span>",
      ) + "<span> · </span><span>esc close</span>"
    );
  }

  // One footer for the frame: a rule, the status row (active lens · stage crumb), and the verb
  // bar for whichever side owns focus. Wide stages paint it under both columns, as the app does.
  function renderFooter() {
    const context = state.tab === "desk" ? "desk" : state.focusProject || state.tab;
    const task = selectedTask();
    const verbs = taskFocus()
      ? pageVerbBar()
      : verbItems(task)
          .map((v) => `<button type="button" class="tsk-verb" data-verb="${esc(v.id)}">${esc(v.label)}</button>`)
          .join("<span> · </span>");
    const footer =
      state.overlay === "quick"
        ? `<div class="tsk-input-row"><span class="tsk-prompt">+</span><input class="tsk-field" id="tsk-add" value="${esc(state.draft)}" placeholder="title  ·  !p project  ·  !t thread" autocomplete="off" /><span class="cursor">█</span></div>
           <div class="foot dim">${state.refuse ? esc(state.refuse) : "enter save · tab details · esc close"}</div>`
        : state.overlay === "search"
          ? `<div class="tsk-input-row"><span class="tsk-prompt">/</span><input class="tsk-field" id="tsk-project-search" value="${esc(state.projectQuery)}" placeholder="search projects" autocomplete="off" /><span class="cursor">█</span></div>
             <div class="foot dim">enter open · esc close</div>`
          : `<div class="tsk-status-row"><button type="button" class="tsk-done-count foot" data-drawer="1">${esc(context)}</button><span class="foot dim tsk-stage-hint">${esc(stageHint())}</span></div>
           <div class="foot dim tsk-verbs">${verbs}</div>
           ${state.copyNotice ? `<div class="foot dim">${esc(state.copyNotice)}</div>` : ""}`;
    return `
      <div class="tsk-foot">
        <div class="foot-rule" aria-hidden="true"></div>
        ${footer}
      </div>`;
  }

  function render() {
    const rows = buildRows();
    ensureSelection(rows);
    const wide = isWideSplit();
    let html;
    if (wide && state.stage === "split") {
      html = `<div class="tsk-wide-split is-split">
           <div class="tsk-board-surface tsk-surface">${renderBoard(rows, false, true)}</div>
           <div class="tsk-rule-column dim" aria-hidden="true"></div>
           ${renderPage(true)}
         </div>${renderFooter()}`;
    } else if (wide && state.stage === "rail") {
      html = `<div class="tsk-wide-split is-rail">
           <div class="tsk-board-surface tsk-rail tsk-surface dim">${renderBoard(rows, true)}</div>
           <div class="tsk-rule-column dim" aria-hidden="true"></div>
           ${renderPage(true)}
         </div>${renderFooter()}`;
    } else if (wide && state.stage === "page") {
      html = `<div class="tsk-wide-split is-page">${renderPage(true)}</div>${renderFooter()}`;
    } else {
      html = renderBoard(rows);
    }
    if (!wide && taskFocus()) html += renderPage();
    if (state.overlay === "help") html += renderHelp();
    if (state.overlay === "palette") html += renderPalette();
    if (state.overlay === "picker") html += renderPicker();
    const activeSearch = document.activeElement?.id === "tsk-project-search";
    const keepKeys =
      document.activeElement === frame || frame.contains(document.activeElement);
    root.innerHTML = html;
    const add = document.getElementById("tsk-add");
    const edit = document.getElementById("tsk-edit");
    const search = document.getElementById("tsk-project-search");
    if (search) {
      search.addEventListener("input", () => {
        state.projectQuery = search.value;
        render();
      });
    }
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
    } else if (search && activeSearch) {
      search.focus();
      search.selectionStart = search.value.length;
      search.selectionEnd = search.value.length;
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

  function openProjectSearchMatch() {
    const row = selectedRow();
    const match = row?.kind === "project" ? row.project : matchingProjectNames()[0];
    if (match) openProject(match);
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
    // Never swallow keys pressed on the landing chrome inside the pane
    // (pane bar, layout toggle, divider): those keep their own keyboard
    // behavior. Buttons inside the board canvas (rows, tabs, chips, verbs,
    // close boxes) also keep native activation, except while the quick-add
    // overlay is open and borrowing the frame's keys for its input.
    const el = e.target;
    if (el !== frame) {
      if (el.closest(".pane-bar, .layout-toggle, [data-divider]")) return;
      if (state.overlay !== "quick" && el.closest("button")) return;
    }
    if (el.id === "tsk-project-search") {
      if (e.key === "Escape") {
        e.preventDefault();
        state.projectQuery = "";
        render();
        frame.focus({ preventScroll: true });
        return;
      }
      if (e.key === "Enter" && !e.ctrlKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        openProjectSearchMatch();
        render();
        return;
      }
      return;
    }
    const wide = isWideSplit();
    const taskPageActive = taskFocus();
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
          openFullPage();
          state.editField = "notes";
          state.editDraft = "";
        }
        render();
      }
      return;
    }

    if (taskPageActive && state.editField) {
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

    if (state.overlay === "search") {
      if (e.key === "Escape") {
        e.preventDefault();
        state.projectQuery = "";
        state.overlay = null;
        render();
        return;
      }
      if (e.key === "Enter") {
        e.preventDefault();
        openProjectSearchMatch();
        state.overlay = null;
        render();
        return;
      }
      if (e.key === "Backspace") {
        e.preventDefault();
        state.projectQuery = state.projectQuery.slice(0, -1);
        render();
        return;
      }
      if (e.key.length === 1 && !e.altKey && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        state.projectQuery += e.key;
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
        if (name === "desk") goTab("desk");
        else openProject(name);
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
    if (taskPageActive && !e.altKey && !e.ctrlKey && ["j", "k", "ArrowDown", "ArrowUp", "1", "2", "3", "+", "z", ":"].includes(e.key)) {
      e.preventDefault();
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      if (taskPageActive) {
        state.overlay = null;
        leaveTaskPage();
      } else if (state.peekId) state.peekId = null;
      else if (state.focusProject) goTab("desk");
      else frame.blur();
      render();
      return;
    }
    if (e.key === "/" && state.tab === "projects" && !state.focusProject) {
      e.preventDefault();
      state.overlay = "search";
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
      goTab(TABS[Number(e.key) - 1][0]);
      render();
      return;
    }
    if (e.key === "p" || e.key === "P") {
      e.preventDefault();
      state.overlay = "picker";
      state.pickerI = 0;
      render();
      return;
    }
    if (e.key === "z" || e.key === "D") {
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
      if (wide) {
        stageRight();
        state.peekId = null;
      } else if (!taskPageActive) {
        state.peekId = state.selectedId;
      }
      render();
      return;
    }
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      if (wide) stageLeft();
      else state.peekId = null;
      render();
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      if (taskPageActive && state.stage === "page") leaveTaskPage();
      else {
        const row = selectedRow();
        if (row?.kind === "project") openProject(row.project);
        else if (row?.kind === "archived") state.archivedOpen = !state.archivedOpen;
        else openFullPage();
      }
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
    if (e.key === "f") {
      e.preventDefault();
      fileSelected();
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
        enterTaskStage();
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
        enterTaskStage();
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
    // A stage A click inside the task column slides to G first, then the control runs.
    const taskColumn = e.target.closest(".tsk-task-column");
    if (taskColumn && isWideSplit() && state.stage === "split") stageRight();
    const close = e.target.closest("[data-close]");
    if (close) {
      state.overlay = null;
      state.paletteQ = "";
      frame.focus();
      render();
      return;
    }
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
    const archivedHeader = e.target.closest("[data-archived-header]");
    if (archivedHeader) {
      state.archivedOpen = !state.archivedOpen;
      render();
      return;
    }
    const projectRow = e.target.closest("[data-project-row]");
    if (projectRow) {
      openProject(projectRow.getAttribute("data-project"));
      render();
      return;
    }
    const group = e.target.closest("[data-collapse]");
    if (group) {
      const now = Date.now();
      const key = group.getAttribute("data-collapse");
      const project = group.getAttribute("data-project");
      if (lastClick.id === key && now - lastClick.at < 350 && project) {
        if (project === "desk") goTab("desk");
        else openProject(project);
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
        state.peekId = null;
        openFullPage();
      } else if (isWideSplit()) {
        // A board or rail row click selects in place: the stage stays put.
        state.selectedId = id;
        state.peekId = null;
        state.overlay = null;
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
    const pageVerb = e.target.closest("[data-page-verb]");
    if (pageVerb) {
      runPageVerb(pageVerb.getAttribute("data-page-verb"));
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
      if (name === "desk") goTab("desk");
      else openProject(name);
      state.overlay = null;
      render();
    }
  });

  frame.addEventListener("keydown", onKey);
  // The landing page's layout toggle asks for a stage directly (full terminal opens in split).
  frame.addEventListener("tsk:set-stage", (e) => {
    const stage = e.detail;
    if (!["board", "split", "rail", "page"].includes(stage)) return;
    if (stage !== "board" && !state.selectedId) return;
    state.stage = stage;
    state.stageOrigin = null;
    state.peekId = null;
    if (state.overlay !== "quick") state.overlay = null;
    render();
  });
  new ResizeObserver(() => render()).observe(root);
  frame.addEventListener("focusin", () => frame.classList.add("is-focused"));
  frame.addEventListener("focusout", (e) => {
    if (!frame.contains(e.relatedTarget)) frame.classList.remove("is-focused");
  });

  render();
})();
