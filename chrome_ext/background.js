/**
 * Displays a notification and logs a message.
 * @param {Function} consoleLogger - The console logging function (e.g., console.log).
 * @param {string} message - The message to log and display.
 * @param {string} title - The notification title.
 */
function popupNotif(consoleLogger, message, title) {
  consoleLogger(message);
  chrome.notifications.create({
    type: "basic",
    iconUrl: "icon.png",
    title: title,
    message: String(message)
  });
}

/**
 * Handles errors by logging and showing a notification.
 * @param {string} errorMessage - The error message to display.
 */
function handleError(errorMessage) {
  popupNotif(console.error, errorMessage, "Error");
}

/**
 * Gets the full HTML content of the current page.
 * @returns {string} The page's HTML content.
 */
function getPageContent() {
  return document.documentElement.outerHTML;
}

/**
 * Records the outcome of the last save to chrome.storage.local, so the side
 * panel (and anything else listening) can show connection/save status
 * without polling.
 * @param {boolean} ok
 * @param {string} message
 */
function recordLastSync(ok, message) {
  chrome.storage.local.set({ lastSync: { ok, message, timestamp: Date.now() } });
}

/**
 * Pushes a captured URL (optionally with page content) to the Maia
 * sync-server's `/urls` endpoint. Requires the base URL and API key to be
 * set on the options page first.
 * @param {{ url: string, title?: string, content?: string }} entry
 */
async function sendToServer(entry) {
  const { baseUrl, apiKey } = await chrome.storage.local.get(["baseUrl", "apiKey"]);
  if (!baseUrl || !apiKey) {
    const message = "Maia is not configured yet — open the extension's options page.";
    handleError(message);
    recordLastSync(false, message);
    return;
  }

  const payload = {
    urls: [
      {
        id: crypto.randomUUID(),
        title: entry.title ?? null,
        url: entry.url,
        source: "chrome-extension",
        tags: "[]",
        isNew: true,
        createdAt: Date.now(),
        ...(entry.content ? { content: entry.content } : {}),
      },
    ],
    deleted: [],
  };

  popupNotif(console.log, `Saving to Maia: ${entry.url}`, "Maia");

  try {
    const response = await fetch(`${baseUrl}/urls`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${apiKey}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(payload),
    });

    if (!response.ok) {
      const message = `Server responded ${response.status}`;
      handleError(message);
      recordLastSync(false, message);
      return;
    }

    popupNotif(console.log, `Saved: ${entry.url}`, "Maia");
    recordLastSync(true, `Saved ${entry.url}`);
  } catch (e) {
    const message = "Network error reaching Maia: " + e.message;
    handleError(message);
    recordLastSync(false, message);
  }
}

/**
 * Initializes context menus when the extension is installed.
 * @listens chrome.runtime.onInstalled
 */
chrome.runtime.onInstalled.addListener(() => {
  chrome.sidePanel.setPanelBehavior({ openPanelOnActionClick: true });
  chrome.contextMenus.create({
    id: "sendUrl",
    title: "Save Page URL to Maia",
    contexts: ["page"]
  });
  chrome.contextMenus.create({
    id: "sendUrlAndContent",
    title: "Save Page URL and Content to Maia",
    contexts: ["page"]
  });
});

/**
 * Handles context menu clicks and keyboard shortcuts to save a URL (with or
 * without page content) to the Maia server.
 * @param {boolean} sendUrlCommand - Whether to save URL only.
 * @param {boolean} sendUrlAndContentCommand - Whether to save URL and content.
 * @param {string} tabUrl - The tab's URL.
 * @param {string} tabTitle - The tab's title.
 * @param {number} tabId - The tab's ID.
 */
function handleCaptureEvent(sendUrlCommand, sendUrlAndContentCommand, tabUrl, tabTitle, tabId) {
  if (sendUrlCommand) {
    sendToServer({ url: tabUrl, title: tabTitle });
  } else if (sendUrlAndContentCommand) {
    chrome.scripting.executeScript({
      target: { tabId: tabId },
      func: getPageContent
    }, (results) => {
      if (chrome.runtime.lastError) {
        handleError("Scripting error: " + chrome.runtime.lastError.message);
        return;
      }
      const content = results[0].result;
      sendToServer({ url: tabUrl, title: tabTitle, content });
    });
  }
}

/**
 * Handles context menu clicks to save URL or URL+content.
 * @param {Object} info - Context menu info (e.g., menuItemId).
 * @param {Object} tab - Current tab info (e.g., url, id, title).
 * @listens chrome.contextMenus.onClicked
 */
chrome.contextMenus.onClicked.addListener((info, tab) => {
  popupNotif(console.log, `Context menu clicked: ${info.menuItemId}`, "Context Menu");
  handleCaptureEvent(
    info.menuItemId === "sendUrl",
    info.menuItemId === "sendUrlAndContent",
    tab.url,
    tab.title,
    tab.id
  );
});

/**
 * Handles keyboard shortcuts to save URL or URL+content.
 * @param {string} command - The command name (e.g., "send-url").
 * @listens chrome.commands.onCommand
 */
chrome.commands.onCommand.addListener((command) => {
  popupNotif(console.log, `Command received: ${command}`, "Keyboard Shortcut");
  chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
    if (!tabs[0]) {
      handleError("No active tab found");
      return;
    }
    handleCaptureEvent(
      command === "send-url",
      command === "send-url-and-content",
      tabs[0].url,
      tabs[0].title,
      tabs[0].id
    );
  });
});

/**
 * Handles messages from popup/sidepanel for bookmarks and reading list.
 * @listens chrome.runtime.onMessage
 */
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === "get_bookmarks") {
    chrome.bookmarks.getTree((bookmarkTree) => {
      if (chrome.runtime.lastError) {
        sendResponse({ status: "error", error: chrome.runtime.lastError.message });
        return;
      }
      chrome.runtime.sendMessage({ type: "bookmarks_response", bookmarks: bookmarkTree }, () => {
        if (chrome.runtime.lastError) {
          console.log("No receiving end for bookmarks_response, ignoring: " + chrome.runtime.lastError.message);
        }
      });
      sendResponse({ status: "sent" });
    });
    return true; // Keep message channel open for async response
  } else if (message.type === "get_reading_list") {
    chrome.readingList.query({}, (items) => {
      if (chrome.runtime.lastError) {
        sendResponse({ status: "error", error: chrome.runtime.lastError.message });
        return;
      }
      chrome.runtime.sendMessage({ type: "reading_list_response", items }, () => {
        if (chrome.runtime.lastError) {
          console.log("No receiving end for reading_list_response, ignoring: " + chrome.runtime.lastError.message);
        }
      });
      sendResponse({ status: "sent" });
    });
    return true; // Keep message channel open for async response
  }
});
