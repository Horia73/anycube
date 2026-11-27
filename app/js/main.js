// The application by itself, as `anycube app` shows it: nothing around it.

import { mount } from "./app.js";

// The private launch URL comes from the native parent, never from an HTTP response.
const launchToken = /^#update=([0-9a-f]{64})$/.exec(location.hash)?.[1];
let desktopToken = launchToken;
try {
  if (launchToken) sessionStorage.setItem("desktop-update", launchToken);
  else desktopToken = sessionStorage.getItem("desktop-update");
} catch {
  // The initial native page can use the fragment even with storage disabled.
}
if (launchToken) {
  history.replaceState(null, "", location.pathname + location.search);
}

function addUpdateCheck(app) {
  if (!app.config.local || !new URLSearchParams(location.search).has("window")) return;
  const token = desktopToken;
  if (!token) return;

  const top = document.getElementById("top-extra");
  const dialog = document.getElementById("updates");
  const status = document.getElementById("update-status");
  const notes = document.getElementById("update-notes");
  const install = document.getElementById("update-install");
  const later = document.getElementById("update-later");
  const progress = document.getElementById("update-progress");
  top.closest(".top").classList.add("local-top");
  top.append(document.getElementById("local-update-button").content.cloneNode(true));

  const trigger = top.querySelector(".update-trigger");
  let state = {};
  let checking;
  let installing = false;
  let monitorId = 0;
  const inProgress = (phase) => ["downloading", "verifying", "installing", "cancelling"].includes(phase);

  async function request(action, body = {}) {
    const response = await fetch(`/api/update/${action}`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-Anycube-Desktop": token },
      body: JSON.stringify(body),
    });
    const result = await response.json();
    if (!response.ok && !result.phase) throw new Error(result.message);
    return result;
  }

  function render(result) {
    state = result;
    const { phase, offer } = result;
    const available = Boolean(offer);
    const label = available ? "Update available" : "Check for updates";
    trigger.classList.toggle("update-available", available);
    trigger.setAttribute("aria-label", label);
    trigger.title = label;
    trigger.querySelector(".wide-only").textContent = label;
    install.hidden = !offer || !["available", "error", "cancelled"].includes(phase);
    install.disabled = installing;
    later.disabled = ["installing", "restarting", "cancelling"].includes(phase);
    later.textContent = ["downloading", "verifying", "cancelling"].includes(phase) ? "Cancel download" : "Later";
    notes.hidden = !offer?.notes;
    const noteText = notes.querySelector("pre");
    if (noteText.textContent !== (offer?.notes ?? "")) noteText.textContent = offer?.notes ?? "";
    progress.hidden = !inProgress(phase);
    if (phase === "downloading") {
      progress.max = offer.bytes;
      progress.value = result.bytes;
    } else {
      progress.removeAttribute("value");
    }
    const phaseText = {
      checking: "Checking for updates…",
      current: "No compatible update is available.",
      available: "Save any work you need before restarting.",
      downloading: "Downloading update…",
      verifying: "Verifying update…",
      installing: "Installing update…",
      restarting: "Reopening anycube…",
      cancelling: "Cancelling download…",
    };
    const message = [
      `Installed version ${result.current ?? app.config.version}${result.current_build ? ` (build ${result.current_build})` : ""}.`,
      offer ? `Version ${offer.version} (build ${offer.build}) is available.` : "",
      result.message || phaseText[phase] || "",
    ].filter(Boolean).join(" ");
    if (status.textContent !== message) status.textContent = message;
  }

  function showError(error) {
    render({ ...state, phase: "error", message: error.message || "Could not contact the desktop app." });
  }

  function check() {
    if (!checking) {
      render({ phase: "checking" });
      checking = request("check").then(render, showError).finally(() => { checking = null; });
    }
    return checking;
  }

  function monitor() {
    const id = ++monitorId;
    async function poll() {
      if (id !== monitorId) return;
      try {
        const result = await request("status");
        if (id !== monitorId) return;
        render(result);
      } catch (error) {
        if (id !== monitorId) return;
        if (!installing) showError(error);
      }
      if (installing || inProgress(state.phase)) setTimeout(poll, 300);
    }
    setTimeout(poll, 300);
  }

  trigger.addEventListener("click", () => {
    dialog.showModal();
    if (!state.offer && !installing) check();
  });
  install.addEventListener("click", async (event) => {
    if (!event.isTrusted || installing || !state.offer) return;
    installing = true;
    install.disabled = true;
    monitor();
    try {
      render(await request("install", { id: state.offer.id }));
    } catch (error) {
      if (state.phase !== "restarting") {
        // A lost response may mean the server already consumed this acceptance.
        try { render(await request("status")); } catch { /* Keep the last visible state. */ }
        if (!inProgress(state.phase) && state.phase !== "restarting") showError(error);
      }
    } finally {
      installing = false;
      ++monitorId;
      install.disabled = false;
      if (inProgress(state.phase)) monitor();
    }
  });
  async function dismiss() {
    if (later.disabled) return;
    if (installing || inProgress(state.phase)) {
      try { render(await request("later")); } catch (error) { showError(error); }
    } else {
      dialog.close();
      try { render(await request("later")); } catch (error) { showError(error); }
    }
  }
  later.addEventListener("click", dismiss);
  dialog.addEventListener("cancel", (event) => {
    event.preventDefault();
    dismiss();
  });

  // Resume a transaction after a page reload without starting another install.
  request("status").then((result) => {
    render(result);
    if (!result.phase) check();
    else if (inProgress(result.phase)) {
      dialog.showModal();
      monitor();
    }
  }, showError);
}

// In a window that is the application (`anycube app` opens "/?window" in a
// browser's application window) the page says when it is there and when it
// goes: a browser on macOS lives on without its window, and the program
// that serves the page ends with the window.
if (new URLSearchParams(location.search).has("window")) {
  // Each loading of the page has a number of its own, so that the "closed"
  // of a page loaded anew is not taken for the new one's.
  const id = Math.floor(Math.random() * 2 ** 50) + 1;
  const tell = (what) => navigator.sendBeacon?.(`/api/window/${what}?id=${id}`, "");
  tell("open");
  addEventListener("pageshow", (e) => e.persisted && tell("open"));
  addEventListener("pagehide", () => tell("closed"));
}

mount(document.getElementById("app")).then(addUpdateCheck);
