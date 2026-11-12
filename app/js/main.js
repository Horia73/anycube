// The application by itself, as `anycube app` shows it: nothing around it.

import { mount } from "./app.js";

const RELEASE_API = "https://api.github.com/repos/Horia73/anycube/releases/latest";

function compareVersions(latest, current) {
  const version = (text) => /^v?(\d+)\.(\d+)\.(\d+)(?:-([\w.-]+))?(?:\+[\w.-]+)?$/.exec(text);
  const candidate = version(latest);
  const installed = version(current);
  if (!candidate || !installed || candidate[4]) throw new Error("Unknown version format");
  for (const part of [1, 2, 3]) {
    const difference = Number(candidate[part]) - Number(installed[part]);
    if (difference) return difference;
  }
  return installed[4] ? 1 : 0;
}

async function latestRelease() {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 8_000);
  try {
    const response = await fetch(RELEASE_API, { credentials: "omit", signal: controller.signal });
    if (!response.ok) throw new Error(`GitHub returned ${response.status}`);
    return (await response.json()).tag_name;
  } finally {
    clearTimeout(timeout);
  }
}

function addUpdateCheck(app) {
  if (!app.config.local) return;
  const top = document.getElementById("top-extra");
  const dialog = document.getElementById("updates");
  const status = document.getElementById("update-status");
  const current = app.config.version;
  top.closest(".top").classList.add("local-top");
  top.append(document.getElementById("local-update-button").content.cloneNode(true));

  const trigger = top.querySelector(".update-trigger");
  let pending;
  let available;
  function check() {
    if (!pending) {
      pending = latestRelease().then((latest) => {
        available = compareVersions(latest, current) > 0 ? latest : null;
        const label = available ? "Update available" : "Check for updates";
        trigger.classList.toggle("update-available", Boolean(available));
        trigger.setAttribute("aria-label", label);
        trigger.title = label;
        trigger.querySelector(".wide-only").textContent = label;
        return available;
      }).finally(() => { pending = null; });
    }
    return pending;
  }

  trigger.addEventListener("click", async () => {
    dialog.showModal();
    status.textContent = `Installed version ${current}. Checking GitHub…`;
    try {
      const latest = available ?? await check();
      status.textContent = latest
        ? `${latest} is available. You have ${current}. Rerun the installer or rebuild from source to update.`
        : `You’re up to date. Installed version ${current}.`;
    } catch {
      status.textContent = `Couldn’t check GitHub. Installed version ${current}.`;
    }
  });
  check().catch(() => {});
}

mount(document.getElementById("app")).then(addUpdateCheck);
