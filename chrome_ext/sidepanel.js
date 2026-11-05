/**
 * Side panel: shows links captured by the extension that are still pending
 * ("unread"), pulled directly from the Maia sync-server's `GET /urls`, plus
 * a status line reflecting the last save/connection outcome recorded by
 * background.js in `chrome.storage.local.lastSync`.
 */

const statusLine = document.getElementById("status-line");
const listEl = document.getElementById("list");
const emptyEl = document.getElementById("empty");

function setStatus(text, kind) {
  statusLine.textContent = text;
  statusLine.className = kind;
}

function formatWhen(ms) {
  try {
    return new Date(ms).toLocaleString();
  } catch {
    return "";
  }
}

async function getConfig() {
  return chrome.storage.local.get(["baseUrl", "apiKey", "lastSync"]);
}

function renderLastSync(lastSync) {
  if (!lastSync) return;
  setStatus(lastSync.message, lastSync.ok ? "ok" : "error");
}

async function fetchPendingUrls() {
  const { baseUrl, apiKey, lastSync } = await getConfig();

  if (!baseUrl || !apiKey) {
    setStatus("Not configured — open the extension's options page to set the server URL and API key.", "unconfigured");
    listEl.innerHTML = "";
    emptyEl.hidden = false;
    return;
  }

  try {
    const response = await fetch(`${baseUrl}/urls?since=0`, {
      headers: { Authorization: `Bearer ${apiKey}` },
    });
    if (!response.ok) {
      setStatus(`Could not load list (server responded ${response.status}).`, "error");
      return;
    }
    const data = await response.json();
    const pending = (data.items || []).filter((item) => item.isNew);
    renderList(pending);
    // A fetch succeeding is itself useful connection information, but it
    // shouldn't clobber a more specific save-outcome message that's still
    // fresh (e.g. from a capture a moment ago).
    if (!lastSync || Date.now() - lastSync.timestamp > 5000) {
      setStatus(`Connected. ${pending.length} pending.`, "ok");
    } else {
      renderLastSync(lastSync);
    }
  } catch (e) {
    setStatus("Network error reaching Maia: " + e.message, "error");
  }
}

function renderList(items) {
  listEl.innerHTML = "";
  emptyEl.hidden = items.length !== 0;

  for (const item of items) {
    const li = document.createElement("li");

    const title = document.createElement("span");
    title.className = "title";
    title.textContent = item.title || item.url;
    li.appendChild(title);

    const url = document.createElement("span");
    url.className = "url";
    url.textContent = `${item.url} · ${formatWhen(item.createdAt)}`;
    li.appendChild(url);

    const actions = document.createElement("div");
    actions.className = "actions";

    const openButton = document.createElement("button");
    openButton.textContent = "Open";
    openButton.addEventListener("click", () => chrome.tabs.create({ url: item.url }));
    actions.appendChild(openButton);

    const readButton = document.createElement("button");
    readButton.textContent = "Mark read";
    readButton.addEventListener("click", () => markRead(item));
    actions.appendChild(readButton);

    li.appendChild(actions);
    listEl.appendChild(li);
  }
}

async function markRead(item) {
  const { baseUrl, apiKey } = await getConfig();
  if (!baseUrl || !apiKey) return;

  try {
    const response = await fetch(`${baseUrl}/urls`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${apiKey}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        urls: [
          {
            id: item.id,
            title: item.title,
            url: item.url,
            source: item.source,
            tags: item.tags,
            isNew: false,
            createdAt: item.createdAt,
          },
        ],
        deleted: [],
      }),
    });
    if (!response.ok) {
      setStatus(`Could not mark as read (server responded ${response.status}).`, "error");
      return;
    }
    await fetchPendingUrls();
  } catch (e) {
    setStatus("Network error reaching Maia: " + e.message, "error");
  }
}

chrome.storage.onChanged.addListener((changes) => {
  if (changes.lastSync) {
    renderLastSync(changes.lastSync.newValue);
    fetchPendingUrls();
  }
  if (changes.baseUrl || changes.apiKey) {
    fetchPendingUrls();
  }
});

fetchPendingUrls();
