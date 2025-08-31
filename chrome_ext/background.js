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
 * Sends a message to the native daemon and handles the response.
 * @param {Object} message - The message to send (e.g., { action, data }).
 */
function sendToDaemon(message) {
  const hostName = "maia.chrome";
  popupNotif(console.log, `Sending to native host: ${JSON.stringify(message)}`, "Native Message");
  chrome.runtime.sendNativeMessage(hostName, message, (response) => {
    if (chrome.runtime.lastError) {
      handleError("Native messaging error: " + chrome.runtime.lastError.message);
      return;
    }
    popupNotif(console.log, `Daemon response: ${JSON.stringify(response)}`, "Daemon Response");
    chrome.runtime.sendMessage({ type: "daemon_response", data: response }, () => {
      if (chrome.runtime.lastError) {
        console.log("No receiving end for daemon_response, ignoring: " + chrome.runtime.lastError.message);
      }
    });
  });
}

/**
 * Initializes context menus when the extension is installed.
 * @listens chrome.runtime.onInstalled
 */
chrome.runtime.onInstalled.addListener(() => {
  chrome.sidePanel.setPanelBehavior({ openPanelOnActionClick: true });
  chrome.contextMenus.create({
    id: "sendUrl",
    title: "Send Page URL to Daemon",
    contexts: ["page"]
  });
  chrome.contextMenus.create({
    id: "sendUrlAndContent",
    title: "Send Page URL and Content to Daemon",
    contexts: ["page"]
  });
});

/**
 * Handles context menu clicks and keyboard shortcuts to send URL or URL+content to daemon.
 * @param {boolean} sendUrlCommand - Whether to send URL.
 * @param {boolean} sendUrlAndContentCommand - Whether to send URL and content.
 * @param {string} tabUrl - The tab's URL.
 * @param {number} tabId - The tab's ID.
 */
function handleEventsForDaemon(sendUrlCommand, sendUrlAndContentCommand, tabUrl, tabId) {
  if (sendUrlCommand) {
    sendToDaemon({ action: "process_url", data: { url: tabUrl } });
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
      sendToDaemon({ action: "process_url_and_content", data: { url: tabUrl, content } });
    });
  }
}

/**
 * Handles context menu clicks to send URL or URL+content to daemon.
 * @param {Object} info - Context menu info (e.g., menuItemId).
 * @param {Object} tab - Current tab info (e.g., url, id).
 * @listens chrome.contextMenus.onClicked
 */
chrome.contextMenus.onClicked.addListener((info, tab) => {
  popupNotif(console.log, `Context menu clicked: ${info.menuItemId}`, "Context Menu");
  handleEventsForDaemon(
    info.menuItemId === "sendUrl",
    info.menuItemId === "sendUrlAndContent",
    tab.url,
    tab.id
  );
});

/**
 * Handles keyboard shortcuts to send URL or URL+content to daemon.
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
    handleEventsForDaemon(
      command === "send-url",
      command === "send-url-and-content",
      tabs[0].url,
      tabs[0].id
    );
  });
});

/**
 * Handles messages from popup for bookmarks and reading list.
 * @listens chrome.runtime.onMessage
 */
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === "send_to_daemon") {
    sendToDaemon(message.data);
    sendResponse({ status: "sent" });
  } else if (message.type === "get_bookmarks") {
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
