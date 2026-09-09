import { TaskSteps } from "./task-steps.js";
import { wrapText } from "./board-wrap.js";
import { parseCapture } from "./capture.js";

(() => {
  const root = document.getElementById("tsk-demo");
  const frame = document.getElementById("board-demo");
  if (!root || !frame) return;

  const WIDE_SPLIT_MIN_COLUMNS = 110;

  const GLYPH = {
    ready: "○",
    started: "●",
    blocked: "■",
    review: "▲",
    done: "✓",
  };

  const TABS = [
    ["desk", "desk"],
    ["project", "selected project"],
    ["projects", "projects"],
  ];
  // Optional fixture data is supplied only by the local parity harness.
  const fixture = JSON.parse(
    document.getElementById("tsk-demo-fixture")?.textContent || "null",
  );
  const clock = () => fixture?.now ?? Date.now();
  const launchProject = () => fixture?.selectedProject || "launchpad";
  const isProjectsThreadView = () =>
    state.tab === "projects" && state.projectView !== null;
  const NOW = clock();
  const MIN = 60 * 1000;
  const HOUR = 60 * MIN;
  const DAY = 24 * HOUR;

  const seed = () => {
    let n = 12;
    const task = (partial) => ({
      id: `t${n}`,
      number: n++,
      notes: "",
      steps: [],
      thread: null,
      project: null,
      archived: false,
      createdAt: NOW - 3 * MIN,
      updatedAt: NOW - 3 * MIN,
      ...partial,
    });
    return [
      task({
        title: "Rotate refresh tokens on every use",
        status: "started",
        project: "launchpad",
        thread: "auth",
        notes:
          "Store a hash of each refresh token and revoke its token family if an old token is reused.",
        steps: [
          {
            id: "sample-0-0",
            text: "Add token-family storage",
            done: true,
          },
          {
            id: "sample-0-1",
            text: "Rotate tokens in the refresh endpoint",
            done: false,
          },
          {
            id: "sample-0-2",
            text: "Test replay and expiry",
            done: false,
          },
        ],
      }),
      task({
        title: "Make webhook delivery idempotent",
        status: "ready",
        project: "launchpad",
        thread: "api",
        notes:
          "Retries can deliver the same event twice. Deduplicate by event ID before applying side effects.",
        steps: [
          {
            id: "sample-1-0",
            text: "Reproduce duplicate delivery",
            done: false,
          },
          {
            id: "sample-1-1",
            text: "Persist processed event IDs",
            done: false,
          },
          {
            id: "sample-1-2",
            text: "Test concurrent retries",
            done: false,
          },
        ],
      }),
      task({
        title: "Investigate slow integration tests",
        status: "ready",
        project: null,
        thread: null,
        notes:
          "The API suite takes twelve minutes in CI. Measure where the time goes before changing the runner.",
        steps: [
          {
            id: "sample-2-0",
            text: "Capture per-test timings",
            done: false,
          },
          {
            id: "sample-2-1",
            text: "Find repeated database setup",
            done: false,
          },
          {
            id: "sample-2-2",
            text: "Compare the next CI run",
            done: false,
          },
        ],
      }),
      task({
        title: "Unblock OIDC login in staging",
        status: "blocked",
        project: "launchpad",
        thread: "auth",
        notes:
          "Waiting for the identity provider's staging client registration. The callback handler is ready.",
        steps: [
          {
            id: "sample-3-0",
            text: "Verify the redirect URI",
            done: true,
          },
          {
            id: "sample-3-1",
            text: "Register the staging client",
            done: false,
          },
          {
            id: "sample-3-2",
            text: "Run the end-to-end login test",
            done: false,
          },
        ],
      }),
      task({
        title: "Review API key scope checks",
        status: "review",
        project: "launchpad",
        thread: "auth",
        notes:
          "The middleware now checks scopes before routing. Review the denial paths and ensure logs never include credentials.",
        steps: [
          {
            id: "sample-4-0",
            text: "Add scope middleware",
            done: true,
          },
          {
            id: "sample-4-1",
            text: "Test missing and insufficient scopes",
            done: false,
          },
          {
            id: "sample-4-2",
            text: "Review the audit log fields",
            done: false,
          },
        ],
      }),
      task({
        title: "Generate the TypeScript API client",
        status: "blocked",
        project: "website",
        thread: "api",
        notes:
          "Waiting for the pagination schema to be finalized before generating the client used by the console.",
        steps: [
          {
            id: "sample-5-0",
            text: "Validate the OpenAPI schema",
            done: true,
          },
          {
            id: "sample-5-1",
            text: "Generate typed request methods",
            done: false,
          },
          {
            id: "sample-5-2",
            text: "Run the client against staging",
            done: false,
          },
        ],
      }),
      task({
        title: "Review error states in the API console",
        status: "review",
        project: "website",
        thread: "api",
        notes:
          "Check how the console handles expired sessions, rate limits, and server errors without losing request input.",
        steps: [
          {
            id: "sample-6-0",
            text: "Handle 401 and 429 responses",
            done: true,
          },
          {
            id: "sample-6-1",
            text: "Preserve the request body on failure",
            done: false,
          },
          {
            id: "sample-6-2",
            text: "Check retry and loading states",
            done: false,
          },
        ],
      }),
      task({
        title: "Add cursor pagination to the events endpoint",
        status: "ready",
        project: "launchpad",
        thread: "api",
        notes:
          "Use a stable cursor for events with matching timestamps. Keep the response contract backwards compatible.",
        steps: [
          {
            id: "sample-7-0",
            text: "Add the cursor query",
            done: false,
          },
          {
            id: "sample-7-1",
            text: "Cover equal-timestamp boundaries",
            done: false,
          },
          {
            id: "sample-7-2",
            text: "Document next-page tokens",
            done: false,
          },
        ],
      }),
      task({
        title: "Add a health check for the worker pool",
        status: "done",
        project: "launchpad",
        thread: "reliability",
        notes:
          "Readiness now fails when workers stop consuming jobs; liveness stays independent.",
        steps: [
          {
            id: "sample-8-0",
            text: "Expose readiness and liveness",
            done: true,
          },
          {
            id: "sample-8-1",
            text: "Test a stalled worker",
            done: true,
          },
        ],
      }),
      task({
        title: "Fix the console build cache",
        status: "done",
        project: "website",
        thread: "tooling",
        notes:
          "Cache dependencies by lockfile and runtime version so CI does not reuse incompatible artifacts.",
        steps: [
          {
            id: "sample-9-0",
            text: "Update the cache key",
            done: true,
          },
          {
            id: "sample-9-1",
            text: "Verify a clean CI build",
            done: true,
          },
        ],
      }),
      task({
        title: "Prototype a GraphQL gateway",
        status: "ready",
        project: "launchpad",
        thread: "api",
        notes:
          "Keep the prototype for reference while the REST API stabilizes.",
        steps: [
          {
            id: "sample-10-0",
            text: "Map the existing endpoints",
            done: false,
          },
          {
            id: "sample-10-1",
            text: "Measure query overhead",
            done: false,
          },
        ],
        archived: true,
      }),
    ];
  };

  const state = {
    tasks: fixture ? structuredClone(fixture.tasks) : seed(),
    tab: "desk",
    // The focused scope is transient, while this is the project selected by tab 2.
    selectedProject: launchProject(),
    focusProject: null,
    projectQuery: "",
    threadFilter: null,
    projectView: null,
    filterI: 0,
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

  const steps = new TaskSteps();

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
    steps.reset();
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
    String(s).replace(
      /[&<>"']/g,
      (c) =>
        ({
          "&": "&amp;",
          "<": "&lt;",
          ">": "&gt;",
          '"': "&quot;",
          "'": "&#39;",
        })[c],
    );

  const age = (ts) => {
    const secs = Math.max(0, Math.floor((clock() - ts) / 1000));
    if (secs < 60) return `${secs}s`;
    if (secs < 3600) return `${Math.floor(secs / 60)}m`;
    if (secs < 86400) return `${Math.floor(secs / 3600)}h`;
    return `${Math.floor(secs / 86400)}d`;
  };

  const projectName = (task) => task.project || "desk";
  const byUpdated = (a, b) => b.updatedAt - a.updatedAt;
  const taskById = (id) => state.tasks.find((t) => t.id === id);

  function pickerOptions() {
    const set = new Set();
    for (const t of state.tasks) if (t.project) set.add(t.project);
    return ["desk", ...[...set].sort()];
  }

  function projectNames() {
    const names = new Set(
      state.tasks.filter((t) => t.project && !t.archived).map((t) => t.project),
    );
    return [...names].sort((a, b) => {
      if (a === launchProject()) return -1;
      if (b === launchProject()) return 1;
      return a.localeCompare(b);
    });
  }

  function projectPath(name) {
    return (
      state.tasks.find((t) => t.project === name)?.scope?.project?.path ||
      `/projects/${name}`
    );
  }
  function matchingProjectNames() {
    const query = state.projectQuery.trim().toLowerCase();
    return projectNames().filter(
      (name) => !query || name.toLowerCase().includes(query),
    );
  }

  function selectedTask() {
    return taskById(state.selectedId) || null;
  }

  let renderColumns = null;
  function terminalColumns() {
    if (renderColumns !== null) return renderColumns;
    const style = getComputedStyle(root);
    const canvas = document.createElement("canvas");
    const context = canvas.getContext("2d");
    context.font = `${style.fontSize} ${style.fontFamily}`;
    const cell = context.measureText("0").width;
    const padding =
      parseFloat(style.paddingLeft) + parseFloat(style.paddingRight);
    return Math.round((root.clientWidth - padding) / cell);
  }

  function isWideSplit() {
    return terminalColumns() >= WIDE_SPLIT_MIN_COLUMNS;
  }

  function fallbackProject() {
    if (state.focusProject) return state.focusProject;
    return null;
  }

  function archivedInScope() {
    const archived = state.tasks
      .filter((t) => t.archived && matchesThread(t))
      .sort(byUpdated);
    if (state.focusProject || isProjectsThreadView()) {
      const inP = (t) =>
        !state.focusProject ||
        (state.focusProject === "desk"
          ? !t.project
          : t.project === state.focusProject);
      return archived.filter(inP);
    }
    return archived;
  }

  function filterOptions() {
    const project = state.focusProject;
    const tasks = state.tasks.filter(
      (t) =>
        !t.archived &&
        t.status !== "done" &&
        (!project || t.project === project),
    );
    const names = [...new Set(tasks.map((t) => t.thread).filter(Boolean))].sort(
      (a, b) =>
        tasks.filter((t) => t.thread === b).length -
          tasks.filter((t) => t.thread === a).length || a.localeCompare(b),
    );
    return [
      {
        value: null,
        label: project ? `All tasks  ${tasks.length}` : "Overview",
      },
      ...names.map((name) => ({
        value: name,
        label: `#${name}  ${tasks.filter((t) => t.thread === name).length}`,
      })),
      ...(project
        ? [
            {
              value: "",
              label: `Without a thread  ${tasks.filter((t) => !t.thread).length}`,
            },
          ]
        : []),
    ];
  }
  function openFilter() {
    if (steps.dirty || steps.editor) return;
    state.overlay = "filter";
    const value = state.focusProject ? state.threadFilter : state.projectView;
    state.filterI = Math.max(
      0,
      filterOptions().findIndex((o) => o.value === value),
    );
  }
  function chooseFilter(index) {
    const option = filterOptions()[index];
    if (!option) return;
    if (state.focusProject) state.threadFilter = option.value;
    else state.projectView = option.value;
    state.overlay = null;
    state.peekId = null;
    state.stage = "board";
  }
  function renderFilter() {
    const title = state.focusProject ? "thread filter" : "projects View";
    return `<div class="tsk-box" role="dialog" aria-label="${title}"><div class="tsk-box-top"><span class="tsk-box-title">${title}</span><button type="button" class="tsk-box-close" data-close="1">[x]</button></div><div class="tsk-box-body">${filterOptions()
      .map(
        (o, i) =>
          `<div class="tsk-pal-row" data-filter-option="${i}"><span class="${i === state.filterI ? "sel-text" : ""}">${i === state.filterI ? "▸" : " "} ${esc(o.label)}</span></div>`,
      )
      .join(
        "",
      )}</div><div class="tsk-box-foot">↑↓ move · enter choose · esc close</div></div>`;
  }
  const matchesThread = (t) =>
    state.focusProject
      ? state.threadFilter === null || (t.thread || "") === state.threadFilter
      : state.tab !== "projects" ||
        state.projectView === null ||
        t.thread === state.projectView;
  function visibleTasks() {
    // Hidden (archived) tasks leave every working view.

    const open = state.tasks.filter(
      (t) => t.status !== "done" && !t.archived && matchesThread(t),
    );
    const done = state.tasks
      .filter((t) => t.status === "done" && !t.archived && matchesThread(t))
      .sort(byUpdated);
    if (state.focusProject || isProjectsThreadView()) {
      const inP = (t) =>
        !state.focusProject ||
        (state.focusProject === "desk"
          ? !t.project
          : t.project === state.focusProject);
      return {
        started: open
          .filter((t) => inP(t) && t.status === "started")
          .sort(byUpdated),
        review: open
          .filter((t) => inP(t) && t.status === "review")
          .sort(byUpdated),
        blocked: open
          .filter((t) => inP(t) && t.status === "blocked")
          .sort(byUpdated),
        ready: open
          .filter((t) => inP(t) && t.status === "ready")
          .sort(byUpdated),
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
      if (v.started.length)
        pushHeader("section", "IN MOTION", v.started.length);
      v.started.forEach((t) => pushTask(t));
      if (v.desk.length || !v.need.length)
        pushHeader("section", "ON DECK · desk", v.desk.length);
      v.desk.forEach((t) => pushTask(t));
      if (state.drawer) {
        pushHeader("section", "DONE", v.done.length);
        v.done.forEach((t) => pushTask(t));
        const archived = archivedInScope();
        if (archived.length) {
          rows.push({
            kind: "archived",
            label: "archived",
            count: archived.length,
            selectable: true,
            id: "nav:archived",
          });
          if (state.archivedOpen)
            archived.forEach((t) =>
              rows.push({
                kind: "task",
                task: t,
                indent: 0,
                selectable: true,
                id: t.id,
                dim: true,
              }),
            );
        }
      }
      return rows;
    }

    if (state.focusProject || isProjectsThreadView()) {
      const v = visibleTasks();
      const need = [...v.review, ...v.blocked].sort(byUpdated);
      if (need.length) {
        pushHeader("section", "NEEDS YOU", need.length);
        need.forEach((t) => pushTask(t));
      }
      if (v.started.length)
        pushHeader("section", "IN MOTION", v.started.length);
      v.started.forEach((t) => pushTask(t));
      pushHeader("section", "ON DECK", v.ready.length);
      v.ready.forEach((t) => pushTask(t));
      if (state.drawer) {
        pushHeader("section", "DONE", v.done.length);
        v.done.forEach((t) => pushTask(t));
        const archived = archivedInScope();
        if (archived.length) {
          rows.push({
            kind: "archived",
            label: "archived",
            count: archived.length,
            selectable: true,
            id: "nav:archived",
          });
          if (state.archivedOpen)
            archived.forEach((t) =>
              rows.push({
                kind: "task",
                task: t,
                indent: 0,
                selectable: true,
                id: t.id,
                dim: true,
              }),
            );
        }
      }
      return rows;
    }

    if (state.tab === "projects") {
      for (const name of matchingProjectNames()) {
        const group = state.tasks.filter(
          (t) =>
            (t.project || "desk") === name &&
            t.status !== "done" &&
            !t.archived,
        );
        const started = group
          .filter((t) => t.status === "started")
          .sort(byUpdated);
        const review = group
          .filter((t) => t.status === "review")
          .sort(byUpdated);
        const blocked = group
          .filter((t) => t.status === "blocked")
          .sort(byUpdated);
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
        const done = state.tasks
          .filter((t) => t.status === "done" && !t.archived && matchesThread(t))
          .sort(byUpdated);
        pushHeader("section", "DONE", done.length);
        done.forEach((t) => pushTask(t));
        const archived = archivedInScope();
        if (archived.length) {
          rows.push({
            kind: "archived",
            label: "archived",
            count: archived.length,
            selectable: true,
            id: "nav:archived",
          });
          if (state.archivedOpen)
            archived.forEach((t) =>
              rows.push({
                kind: "task",
                task: t,
                indent: 0,
                selectable: true,
                id: t.id,
                dim: true,
              }),
            );
        }
      }
      return rows;
    }

    if (state.drawer) {
      const done = state.tasks
        .filter((t) => t.status === "done" && !t.archived && matchesThread(t))
        .sort(byUpdated);
      pushHeader("section", "DONE", done.length);
      done.forEach((t) => pushTask(t));
      const archived = archivedInScope();
      if (archived.length) {
        rows.push({
          kind: "archived",
          label: "archived",
          count: archived.length,
          selectable: true,
          id: "nav:archived",
        });
        if (state.archivedOpen)
          archived.forEach((t) =>
            rows.push({
              kind: "task",
              task: t,
              indent: 0,
              selectable: true,
              id: t.id,
              dim: true,
            }),
          );
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
    if (!state.focusProject && task.project) bits.push(projectName(task));
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
      items.push({
        id: "block",
        label: task.status === "blocked" ? "b unblock" : "b block",
      });
    }
    items.push(
      { id: "capture", label: "+ add" },
      { id: "help", label: "? help" },
    );
    return items;
  }

  function runVerb(id) {
    if (id === "search") state.overlay = "search";
    if (id === "open" && state.selectedId) {
      const row = selectedRow();
      if (row?.kind === "project") openProject(row.project);
      else openFullPage();
    }
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

  function paletteCommands() {
    const q = state.paletteQ.trim().toLowerCase();
    const all = [
      {
        id: "ready",
        label: "set status: ready",
        run: () => setStatus("ready"),
      },
      {
        id: "started",
        label: "set status: started",
        run: () => setStatus("started"),
      },
      {
        id: "blocked",
        label: "set status: blocked",
        run: () => setStatus("blocked"),
      },
      {
        id: "review",
        label: "set status: review",
        run: () => setStatus("review"),
      },
      { id: "done", label: "set status: done", run: () => setStatus("done") },
      { id: "desk", label: "go desk", run: () => goTab("desk") },
      { id: "projects", label: "go projects", run: () => goTab("projects") },
      {
        id: "project",
        label: "go selected project",
        run: () => goTab("project"),
      },
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
    task.updatedAt = clock();
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
    if (steps.dirty || steps.editor) return;
    state.threadFilter = null;
    state.tab = tab;
    state.focusProject = tab === "project" ? state.selectedProject : null;
    state.projectQuery = "";
    state.peekId = null;
    state.overlay = null;
    state.stage = "board";
    state.stageOrigin = null;
  }

  function openProject(name) {
    if (steps.dirty || steps.editor) return;
    if (!name || name === "desk") {
      goTab("desk");
      return;
    }
    state.threadFilter = null;
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
    return (
      buildRows().find(
        (row) => row.selectable && row.id === state.selectedId,
      ) || null
    );
  }

  function toggleAllGroups() {
    if (state.focusProject || state.tab !== "projects") return;
    const groups = buildRows().filter(
      (row) => row.kind === "group" && !row.indent,
    );
    const collapse = groups.some(
      (row) => !state.collapsed.has(row.collapseKey),
    );
    for (const row of groups) {
      if (collapse) state.collapsed.add(row.collapseKey);
      else state.collapsed.delete(row.collapseKey);
    }
  }

  function resetDemo() {
    state.tasks = seed();
    state.threadFilter = null;
    state.projectView = null;
    state.filterI = 0;
    state.tab = "desk";
    state.selectedProject = "launchpad";
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
          <div>steps: tab / shift+tab select | enter toggle | a add</div>
          <div>e rename selected step | x twice delete | shift+enter save | esc cancel</div>
          <div class="dim">app needs ctrl on verbs · demo also accepts bare keys</div>
        </div>
        <div class="tsk-box-foot">esc close</div>
      </div>`;
  }

  function renderPalette() {
    const cmds = paletteCommands();
    if (state.paletteI >= cmds.length)
      state.paletteI = Math.max(0, cmds.length - 1);
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
    const opts = pickerOptions();
    if (state.pickerI >= opts.length)
      state.pickerI = Math.max(0, opts.length - 1);
    const list = opts
      .map((name, i) => {
        const mark = i === state.pickerI ? "▸" : " ";
        const cls = i === state.pickerI ? "sel-text" : "";
        return `<div class="tsk-pal-row" data-pick="${esc(name)}"><span class="${cls}">${mark} ${esc(name)}</span></div>`;
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
    if (id === "edit" && steps.selected && steps.selected !== "add") {
      steps.begin(task, steps.selected);
      return;
    }
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
  function taskColumnWidth() {
    const cols = terminalColumns();
    return state.stage === "split"
      ? cols - Math.floor(cols * 0.4) - 2
      : state.stage === "rail"
        ? cols - 34
        : cols;
  }

  function renderPage(embedded = false) {
    const task = selectedTask();
    const focused = taskFocus();
    if (!task) {
      if (embedded) {
        return `<div class="tsk-task-column tsk-surface" aria-label="task column"><div class="tsk-task-header dim"><span class="sec">no task</span></div><div class="tsk-task-rule" aria-hidden="true"></div><div class="tsk-task-surface"><div class="dim">  select a task to preview it here</div></div></div>`;
      }
      return `<div class="tsk-overlay"><div class="dim">no task</div><div class="dim">  select a task to preview it here</div></div>`;
    }
    steps.bind(task);
    const editing = state.editField;
    const narrow = !isWideSplit();
    const stateSlot = steps.editor
      ? "editing step"
      : steps.dirty
        ? "unsaved"
        : editing
          ? `editing ${editing}`
          : narrow
            ? task.status
            : `${task.status} · ${projectName(task)}`;
    const headTitle =
      editing === "title"
        ? `<input class="tsk-field" id="tsk-edit" value="${esc(state.editDraft)}" />`
        : esc(task.title);
    const glyph = GLYPH[task.status] || "○";
    let header = `<div class="tsk-task-header ${focused ? "is-bold" : "dim"}"><span class="glyph">${glyph}</span> <span class="tsk-task-id" data-copy-task="${esc(task.id)}" title="copy T${task.number}">T${task.number}</span> <span class="sec">${headTitle}</span><span class="tsk-state-slot">${esc(stateSlot)}</span></div><div class="tsk-task-rule" aria-hidden="true"></div>`;
    if (narrow && !editing) {
      const room = terminalColumns() - 9 - stateSlot.length;
      const titleRows = wrapText(task.title, room);
      const headerStatus =
        titleRows[0].length + 8 + stateSlot.length <= terminalColumns() - 2
          ? stateSlot
          : "";
      header = `<div class="tsk-task-header tsk-narrow-header"><span class="tsk-page-prefix">${glyph} <span class="tsk-task-id" data-copy-task="${esc(task.id)}">T${task.number}</span> </span><span class="tsk-page-title">${titleRows.map((line) => `<span>${esc(line)}</span>`).join("")}</span><span class="tsk-state-slot">${esc(headerStatus)}</span></div><div class="tsk-task-rule"></div>`;
    }
    const notes =
      editing === "notes"
        ? `<textarea class="tsk-field tsk-notes" id="tsk-edit">${esc(state.editDraft)}</textarea>`
        : `<div class="tsk-page-notes">${wrapText(
            task.notes || "no notes yet",
            taskColumnWidth() - 6,
          )
            .map((line) => `<span>${esc(line) || " "}</span>`)
            .join("")}</div>`;
    const stepRows = steps.rows(task);
    const inlineEditor = `<textarea id="tsk-step-edit" class="tsk-field" aria-label="Step text" rows="${wrapText(steps.editor?.text ?? "", taskColumnWidth() - 8).length}">${esc(steps.editor?.text ?? "")}</textarea><span class="tsk-step-refusal">${esc(steps.refusal)}</span>`;
    const stepList = `<div class="tsk-steps"><div class="tsk-steps-heading dim">steps ${stepRows.filter((step) => step.done).length}/${stepRows.length}</div>${stepRows
      .map(
        (step) =>
          `<div class="tsk-step" data-step="${esc(step.id)}" role="option" aria-selected="${steps.selected === step.id}"><span class="tsk-step-glyph">${steps.selected === step.id ? "▸ " : "  "}${steps.marked === step.id ? "✗" : step.done ? "✓" : "▪"} </span>${
            steps.editor?.id === step.id
              ? inlineEditor
              : `<span class="tsk-step-text">${wrapText(
                  step.text,
                  narrow
                    ? terminalColumns() - 8
                    : Math.max(8, taskColumnWidth() - 8),
                )
                  .map((line) => `<span>${esc(line)}</span>`)
                  .join("")}</span>`
          }</div>`,
      )
      .join(
        "",
      )}${steps.editor && !steps.editor.id ? `<div class="tsk-step-new">${inlineEditor}</div>` : `<button type="button" class="tsk-step-add dim" data-step-add="1">   + step</button>`}</div>`;
    const meta = `<div class="tsk-page-meta dim">${narrow ? `${esc(projectName(task))} · ` : ""}${task.thread ? `#${esc(task.thread)} · ` : ""}created ${esc(age(task.createdAt))} ago · updated ${esc(age(task.updatedAt))} ago</div>`;
    return `<div class="tsk-task-column tsk-surface ${narrow ? "is-narrow" : ""}" aria-label="T${task.number} task column" data-status="${esc(task.status)}" data-edit-state="${steps.editor ? "editing" : steps.dirty ? "unsaved" : "view"}">${header}<div class="tsk-task-surface tsk-page">${notes}${stepList}</div>${meta}</div>`;
  }

  function stageHint() {
    if (!isWideSplit()) return "";
    if (state.editField) return "shift+enter save · esc cancel";
    if (state.stage === "board") return "→ task pane";
    if (state.stage === "split")
      return "board ▸ task    → task · ← close · enter open";
    if (state.stage === "rail") return "board ◂ task    ← board · → full page";
    return "← rail";
  }

  // Same overflow rule as the native threads_cell: reserve room for hidden names.
  function threadCell(threads, width) {
    let out = "",
      shown = 0;
    for (const thread of threads) {
      const candidate = out ? out + " #" + thread : "#" + thread;
      const remaining = threads.length - shown - 1;
      const suffix = remaining ? "  +" + remaining : "";
      if (candidate.length + suffix.length > width) break;
      out = candidate;
      shown++;
    }
    if (!shown)
      return threads.length ? ("+" + threads.length).slice(0, width) : "";
    return (
      out + (shown < threads.length ? "  +" + (threads.length - shown) : "")
    );
  }

  function renderBoard(rows, rail = false, bare = false) {
    const tabs = TABS.map(([tab, label]) => {
      const on = state.tab === tab;
      const text = tab === "project" ? state.selectedProject : label;
      return `<span class="tsk-tab-group ${on ? "is-on" : ""}"><button type="button" class="tsk-tab ${on ? "is-on" : ""}" data-tab="${tab}">${esc(text)}</button>${tab === "project" ? `<button class="tsk-tab-arrow" data-chip="1" aria-label="choose project">▾</button>` : ""}</span>`;
    }).join(`<span class="dim"> · </span>`);
    const filter = state.focusProject
      ? state.threadFilter === null
        ? "all"
        : state.threadFilter === ""
          ? "without a thread"
          : `#${state.threadFilter}`
      : state.tab === "projects"
        ? state.projectView === null
          ? "Overview"
          : `#${state.projectView}`
        : null;
    const control =
      filter === null
        ? ""
        : `<button class="tsk-view-control dim" data-filter="1">${esc(filter)} ▾</button>`;
    const index =
      state.tab === "projects" &&
      state.projectView === null &&
      !state.focusProject;
    const showThreads = terminalColumns() >= 100;
    const available = terminalColumns() - 34;
    const nameWidth = Math.max(24, Math.floor(available * 0.4));
    const threadWidth = Math.max(0, available - nameWidth - 2);
    const count = (n) => n || "·";
    const body = rows
      .map((row) => {
        if (row.kind === "section" || row.kind === "sub") {
          const cls = row.kind === "sub" ? "tsk-sub" : "tsk-sec";
          return `<div class="${cls}"><span class="sec">${esc(row.label)}</span><span class="rule" aria-hidden="true"></span><span class="count">${row.count}</span></div>`;
        }
        if (row.kind === "project") {
          const selected = row.id === state.selectedId;
          const threads = [
            ...new Set(
              state.tasks
                .filter(
                  (t) =>
                    t.project === row.project &&
                    !t.archived &&
                    t.status !== "done",
                )
                .map((t) => t.thread)
                .filter(Boolean),
            ),
          ].sort();
          return `<button type="button" class="tsk-project-row ${selected ? "is-selected" : ""}" data-project-row="${esc(row.project)}" data-nav-id="${esc(row.id)}"><span class="tsk-project-name">${selected ? "▸" : " "} <span>${esc(row.label)}</span>${row.project === launchProject() ? `<span class="dim"> · here</span>` : ""}</span>${showThreads ? `<span class="dim tsk-project-threads">${esc(threadCell(threads, threadWidth))}</span>` : ""}<span class="${row.needs ? "is-bold" : "dim"}">${count(row.needs)}</span><span>${count(row.motion)}</span><span class="dim">${count(row.ready)}</span></button>`;
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
        const glyph = GLYPH[task.status] || "○";
        const indent = "  ".repeat(row.indent || 0);
        if (rail) {
          const prefix = `${selected ? "▸ " : "  "}${glyph} `;
          const lines = wrapText(
            task.title,
            32 - 1 - 4 - `T${task.number} `.length,
          );
          return `<button type="button" class="tsk-row tsk-rail-row" data-task="${task.id}"><span class="tsk-row-main">${prefix}<span class="tsk-task-id" data-copy-task="${esc(task.id)}">T${task.number}</span> ${esc(lines[0])}${lines
            .slice(1)
            .map(
              (line) =>
                `<span class="tsk-rail-continuation">    ${esc(line)}</span>`,
            )
            .join("")}</span></button>`;
        }
        const columns =
          isWideSplit() && state.stage === "split"
            ? Math.floor(terminalColumns() * 0.4)
            : terminalColumns();
        const titleLines = wrapText(
          task.title,
          columns - 2 - 4 - `T${task.number} `.length,
        );
        const title = titleLines
          .map((line) => `<span class="tsk-title-line">${esc(line)}</span>`)
          .join("");
        const noteLines = wrapText(
          (task.notes || "").trim() || "no notes yet",
          columns - 7,
        );
        const label = metaFor(task);
        const peek =
          state.peekId === task.id && !isWideSplit()
            ? [
                ...noteLines
                  .slice(0, 5)
                  .map(
                    (line) =>
                      `<div class="tsk-peek dim">    │ ${esc(line)}</div>`,
                  ),
                ...(noteLines.length > 5
                  ? [
                      `<div class="tsk-peek dim">    │ … ${noteLines.length - 5} more lines</div>`,
                    ]
                  : []),
                ...(label
                  ? wrapText(label, columns - 9).map(
                      (line, i) =>
                        `<div class="tsk-attribution dim">${i ? "       " : "    └─ "}${esc(line)}</div>`,
                    )
                  : [`<div class="tsk-peek dim">    └</div>`]),
              ].join("")
            : "";
        const dimRow = row.dim ? "dim" : "";
        return `<button type="button" class="tsk-row ${dimRow} ${selected ? "is-sel" : ""} ${flash ? "is-flash" : ""}" data-task="${task.id}"><span class="tsk-row-main"><span class="tsk-row-prefix">${selected ? "▸ " : "  "}<span class="tsk-row-glyph">${glyph}</span> <span class="tsk-task-id" data-copy-task="${esc(task.id)}" title="copy T${task.number}">T${task.number}</span> </span><span class="tsk-row-title">${title}</span></span></button>${peek}`;
      })
      .join("");

    const column = `
      <div class="tsk-tabs">${tabs}${control}</div>
      <div class="tsk-list ${index ? "tsk-project-table" : ""} ${showThreads ? "with-threads" : ""}" style="--project-name-width:${nameWidth + 2}ch;--project-thread-width:${threadWidth + 2}ch">${index ? `<div class="tsk-project-legend"><span>  PROJECT</span>${showThreads ? "<span>THREADS</span>" : ""}<span>NEEDS YOU</span><span>IN MOTION</span><span>READY</span></div>` : ""}${body || `<div class="dim">  nothing here</div>`}</div>`;
    // Wide stages paint one shared footer under both columns, so a column omits its own.
    return rail || bare ? column : column + renderFooter();
  }

  const PAGE_VERBS = [
    { id: "edit", label: "e edit" },
    { id: "done", label: "d done" },
    { id: "block", label: "b block" },
  ];

  function pageVerbBar() {
    if (steps.editor || steps.dirty) return "shift+enter save · esc cancel";
    return (
      PAGE_VERBS.map(
        (v) =>
          `<button type="button" class="tsk-verb" data-page-verb="${v.id}">${v.label}</button>`,
      ).join("<span> · </span>") + "<span> · </span><span>esc close</span>"
    );
  }

  // One footer for the frame: a rule, the status row (active lens · stage crumb), and the verb
  // bar for whichever side owns focus. Wide stages paint it under both columns, as the app does.
  function renderFooter() {
    const context =
      state.tab === "projects" && state.projectView === null
        ? selectedRow()?.project
          ? projectPath(selectedRow().project)
          : "projects"
        : state.tab === "desk"
          ? "desk"
          : state.focusProject
            ? `${state.focusProject}${state.threadFilter ? ` · #${state.threadFilter}` : ""}`
            : state.tab;
    const task = selectedTask();
    const verbs = taskFocus()
      ? pageVerbBar()
      : verbItems(task)
          .map(
            (v) =>
              `<button type="button" class="tsk-verb" data-verb="${esc(v.id)}">${esc(v.label)}</button>`,
          )
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
    renderColumns = terminalColumns();
    try {
      const rows = buildRows();
      ensureSelection(rows);
      const wide = isWideSplit();
      let html;
      if (wide && state.stage === "split") {
        html = `<div class="tsk-wide-split is-split" style="grid-template-columns:${Math.floor(terminalColumns() * 0.4)}ch 2ch minmax(0,1fr)">
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
      } else if (taskFocus()) {
        html = `<div class="tsk-single-task">${renderPage(true)}</div>${renderFooter()}`;
      } else {
        html = renderBoard(rows);
      }
      if (state.overlay === "help") html += renderHelp();
      if (state.overlay === "palette") html += renderPalette();
      if (state.overlay === "picker") html += renderPicker();
      if (state.overlay === "filter") html += renderFilter();
      const oldScroll = root.querySelector(".tsk-task-surface")?.scrollTop ?? 0;
      const activeSearch = document.activeElement?.id === "tsk-project-search";
      const keepKeys =
        document.activeElement === frame ||
        frame.contains(document.activeElement);
      root.innerHTML = html;
      const content = root.querySelector(".tsk-task-surface");
      if (content) content.scrollTop = oldScroll;
      const stepInput = root.querySelector("#tsk-step-edit");
      if (stepInput) {
        stepInput.addEventListener("input", () => {
          steps.editor.text = stepInput.value;
          steps.refusal = "";
          root.querySelector(".tsk-step-refusal").textContent = "";
          stepInput.rows = wrapText(
            stepInput.value,
            taskColumnWidth() - 8,
          ).length;
        });
        stepInput.focus({ preventScroll: true });
        stepInput.setSelectionRange(
          stepInput.value.length,
          stepInput.value.length,
        );
        stepInput.scrollIntoView({ block: "nearest" });
      }
      const add = document.getElementById("tsk-add");
      const edit = document.getElementById("tsk-edit");
      const search = document.getElementById("tsk-project-search");
      if (search) {
        search.addEventListener("input", () => {
          state.projectQuery = search.value;
          render();
        });
      }
      if (!stepInput) {
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
      }
      frame.classList.toggle(
        "is-focused",
        document.activeElement === frame ||
          frame.contains(document.activeElement),
      );
    } finally {
      renderColumns = null;
    }
  }

  function move(delta) {
    if (steps.dirty || steps.editor) return;
    const ids = selectableIds(buildRows());
    if (!ids.length) return;
    let i = ids.indexOf(state.selectedId);
    if (i < 0) i = 0;
    i = (i + delta + ids.length) % ids.length;
    state.selectedId = ids[i];
    state.peekId =
      state.peekId && state.peekId === state.selectedId ? state.peekId : null;
  }

  function openProjectSearchMatch() {
    const row = selectedRow();
    const match =
      row?.kind === "project" ? row.project : matchingProjectNames()[0];
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
    task.updatedAt = clock();
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

    if (taskPageActive && !state.overlay && !state.editField) {
      const task = selectedTask();
      const bare = !e.ctrlKey && !e.metaKey && !e.altKey;
      if (steps.editor) {
        if (e.key === "Escape") {
          e.preventDefault();
          steps.cancel();
          render();
        } else if (e.key === "Enter" && bare) {
          e.preventDefault();
          const persists = !steps.editor.id || e.shiftKey;
          if (steps.save(task, e.shiftKey) && persists)
            task.updatedAt = clock();
          render();
        }
        return;
      }
      if (steps.dirty && e.key === "Escape") {
        e.preventDefault();
        steps.cancel();
        render();
        return;
      }
      if (steps.dirty && e.key === "Enter" && e.shiftKey && bare) {
        e.preventDefault();
        steps.save(task, true);
        task.updatedAt = clock();
        render();
        return;
      }
      if (bare && ["Tab", "ArrowDown", "ArrowUp", "j", "k"].includes(e.key)) {
        e.preventDefault();
        steps.move(
          task,
          e.shiftKey || ["ArrowUp", "k"].includes(e.key) ? -1 : 1,
        );
        render();
        root
          .querySelector(`[data-step="${steps.selected}"]`)
          ?.scrollIntoView({ block: "nearest" });
        return;
      }
      if (bare && e.key === "Enter" && steps.selected) {
        e.preventDefault();
        if (steps.selected === "add") steps.begin(task);
        else {
          steps.toggle(task);
          task.updatedAt = clock();
        }
        render();
        return;
      }
      if (bare && e.key === "a") {
        e.preventDefault();
        steps.begin(task);
        render();
        return;
      }
      if (bare && e.key === "e" && steps.selected && steps.selected !== "add") {
        e.preventDefault();
        steps.begin(task, steps.selected);
        render();
        return;
      }
      if (bare && e.key === "x" && steps.selected && steps.selected !== "add") {
        e.preventDefault();
        if (steps.remove(task)) task.updatedAt = clock();
        render();
        return;
      }
      steps.marked = null;
      if (steps.dirty && ["e", "n", "f", "p"].includes(e.key)) {
        e.preventDefault();
        return;
      }
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

    if (state.overlay === "filter") {
      e.preventDefault();
      if (e.key === "Escape") state.overlay = null;
      else if (e.key === "Enter") chooseFilter(state.filterI);
      else if (["ArrowDown", "j"].includes(e.key))
        state.filterI = Math.min(filterOptions().length - 1, state.filterI + 1);
      else if (["ArrowUp", "k"].includes(e.key))
        state.filterI = Math.max(0, state.filterI - 1);
      render();
      return;
    }
    if (
      !state.overlay &&
      !taskPageActive &&
      ((e.key === "t" && state.focusProject) ||
        (e.key === "v" && state.tab === "projects"))
    ) {
      e.preventDefault();
      openFilter();
      render();
      return;
    }
    if (state.overlay === "picker") {
      const opts = pickerOptions();
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
    if (
      taskPageActive &&
      !e.altKey &&
      !e.ctrlKey &&
      [
        "j",
        "k",
        "ArrowDown",
        "ArrowUp",
        "1",
        "2",
        "3",
        "+",
        "z",
        "D",
        ":",
      ].includes(e.key)
    ) {
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
    if (e.key === "?") {
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
      state.pickerI = Math.max(
        0,
        pickerOptions().indexOf(state.selectedProject),
      );
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
        else if (row?.kind === "archived")
          state.archivedOpen = !state.archivedOpen;
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
    const filterControl = e.target.closest("[data-filter]");
    if (filterControl) {
      openFilter();
      render();
      return;
    }
    const filterOption = e.target.closest("[data-filter-option]");
    if (filterOption) {
      chooseFilter(Number(filterOption.dataset.filterOption));
      render();
      return;
    }
    const stepTarget = e.target.closest("[data-step], [data-step-add]");
    if (stepTarget && e.target.id !== "tsk-step-edit") {
      const task = selectedTask();
      if (steps.editor && !steps.editor.text.trim() && !steps.editor.id)
        steps.cancel();
      else if (steps.editor && !steps.save(task, false)) {
        render();
        return;
      }
      enterTaskStage();
      if (stepTarget.hasAttribute("data-step-add")) steps.begin(task);
      else {
        steps.selected = stepTarget.dataset.step;
        steps.marked = null;
      }
      render();
      return;
    }
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
      const task = state.tasks.find(
        (item) => item.id === copy.getAttribute("data-copy-task"),
      );
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
      state.pickerI = Math.max(
        0,
        pickerOptions().indexOf(state.selectedProject),
      );
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
      const id = projectRow.dataset.navId,
        now = Date.now();
      if (lastClick.id === id && now - lastClick.at < 350)
        openProject(projectRow.dataset.projectRow);
      else state.selectedId = id;
      lastClick = { id, at: now };
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
      if ((steps.dirty || steps.editor) && id !== state.selectedId) return;
      const now = Date.now();
      if (lastClick.id === id && now - lastClick.at < 350) {
        state.selectedId = id;
        state.peekId = null;
        openFullPage();
      } else if (isWideSplit()) {
        if (state.stage === "rail") state.stage = "split";
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
      const hit = paletteCommands().find(
        (c) => c.id === cmd.getAttribute("data-cmd"),
      );
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
  frame.dispatchEvent(new CustomEvent("tsk:ready"));
})();
