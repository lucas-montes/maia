/**
 * Options page: lets the user point the extension at their sync-server and
 * requests host permission for that specific origin (declared as
 * `optional_host_permissions` in manifest.json, since the server's address
 * is user-configured rather than fixed at install time).
 */

document.addEventListener("DOMContentLoaded", async () => {
  const baseUrlInput = document.getElementById("baseUrl");
  const apiKeyInput = document.getElementById("apiKey");
  const statusEl = document.getElementById("status");
  const saveButton = document.getElementById("save");

  const stored = await chrome.storage.local.get(["baseUrl", "apiKey"]);
  if (stored.baseUrl) baseUrlInput.value = stored.baseUrl;
  if (stored.apiKey) apiKeyInput.value = stored.apiKey;

  function showStatus(message, isError) {
    statusEl.textContent = message;
    statusEl.className = isError ? "error" : "ok";
  }

  saveButton.addEventListener("click", async () => {
    const baseUrl = baseUrlInput.value.trim().replace(/\/+$/, "");
    const apiKey = apiKeyInput.value.trim();

    if (!baseUrl || !apiKey) {
      showStatus("Both the server URL and API key are required.", true);
      return;
    }

    let origin;
    try {
      origin = new URL(baseUrl).origin + "/*";
    } catch {
      showStatus("That doesn't look like a valid URL.", true);
      return;
    }

    try {
      const granted = await chrome.permissions.request({ origins: [origin] });
      if (!granted) {
        showStatus("Permission to reach that server was not granted.", true);
        return;
      }
    } catch (e) {
      showStatus("Permission request failed: " + e.message, true);
      return;
    }

    await chrome.storage.local.set({ baseUrl, apiKey });
    showStatus("Saved.", false);
  });
});
