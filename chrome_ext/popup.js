/**
 * Initializes the popup and sets up button event listeners.
 * @listens DOMContentLoaded
 */
document.addEventListener("DOMContentLoaded", () => {
  const responseElement = document.getElementById("response");
  const openSidebarButton = document.getElementById("openSidebar");
  const readBookmarksButton = document.getElementById("readBookmarks");
  const readReadingListButton = document.getElementById("readReadingList");

  /**
   * Updates the popup with daemon responses.
   * @param {Object} message - The message from background.js.
   * @param {Object} sender - The sender info.
   * @param {Function} sendResponse - Callback to respond.
   */
  chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (message.type === "daemon_response") {
      responseElement.textContent = JSON.stringify(message.data, null, 2);
    } else if (message.type === "bookmarks_response") {
      responseElement.textContent = JSON.stringify(message.bookmarks, null, 2);
    } else if (message.type === "reading_list_response") {
      responseElement.textContent = JSON.stringify(message.items, null, 2);
    }
  });

  // Open Sidebar
  openSidebarButton.addEventListener("click", () => {
    chrome.windows.getCurrent((window) => {
      if (chrome.runtime.lastError || !window) {
        responseElement.textContent = "Error: Cannot determine current window";
        return;
      }
      chrome.sidePanel.open({ windowId: window.id }, () => {
        if (chrome.runtime.lastError) {
          responseElement.textContent = "Error opening sidebar: " + chrome.runtime.lastError.message;
          return;
        }
        responseElement.textContent = "Sidebar opened";
      });
    });
  });

  // Read Bookmarks
  readBookmarksButton.addEventListener("click", () => {
    chrome.runtime.sendMessage({ type: "get_bookmarks" }, (response) => {
      if (chrome.runtime.lastError) {
        responseElement.textContent = "Error reading bookmarks: " + chrome.runtime.lastError.message;
        return;
      }
      responseElement.textContent = response.status === "sent" ? "Bookmarks requested" : "Error: " + response.error;
    });
  });

  // Read Reading List
  readReadingListButton.addEventListener("click", () => {
    chrome.runtime.sendMessage({ type: "get_reading_list" }, (response) => {
      if (chrome.runtime.lastError) {
        responseElement.textContent = "Error reading reading list: " + chrome.runtime.lastError.message;
        return;
      }
      responseElement.textContent = response.status === "sent" ? "Reading list requested" : "Error: " + response.error;
    });
  });
});
