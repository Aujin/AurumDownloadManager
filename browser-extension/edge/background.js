const HOST = "com.aujin.aurumdownloadmanager";
const PREFIX = "aurum-download-";
const processing = new Set();
const browserName = navigator.userAgent.includes("Edg/") ? "Edge" : "Chrome";

function native(message) {
  return new Promise((resolve, reject) => {
    chrome.runtime.sendNativeMessage(HOST, message, (answer) => {
      const error = chrome.runtime.lastError;
      if (error) reject(new Error(error.message));
      else if (!answer?.ok) reject(new Error(answer?.error || "桌面连接失败"));
      else resolve(answer);
    });
  });
}

async function mode() {
  const saved = await chrome.storage.local.get("mode");
  if (["ask", "always", "browser"].includes(saved.mode)) return saved.mode;
  try { return (await native({ type: "settings", browser: navigator.userAgent.includes("Edg/") ? "edge" : "chrome" })).mode; }
  catch { return "browser"; }
}

async function pending(id) {
  const value = await chrome.storage.local.get(`pending-${id}`);
  return value[`pending-${id}`];
}

async function finish(id, useAurum) {
  if (processing.has(id)) return;
  processing.add(id);
  try {
  const item = await pending(id);
  if (!item) return;
  await chrome.storage.local.remove(`pending-${id}`);
  await chrome.alarms.clear(`resume-${id}`);
  try {
    if (useAurum) {
      await native({ type: "enqueue", url: item.url });
      await chrome.downloads.cancel(id);
      return;
    }
    await chrome.downloads.resume(id);
  } catch (error) {
    try { await chrome.downloads.resume(id); } catch { /* Download may already be gone. */ }
    chrome.notifications.create(`${PREFIX}error-${Date.now()}`, {
      type: "basic", iconUrl: "icon128.png", title: "Aurum 未能接收下载",
      message: String(error.message || error),
    });
  }
  } finally { processing.delete(id); }
}

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({ id: "download-with-aurum", title: "使用 Aurum 下载链接", contexts: ["link"] });
});

chrome.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId !== "download-with-aurum" || !info.linkUrl) return;
  try { await native({ type: "enqueue", url: info.linkUrl }); }
  catch (error) {
    chrome.notifications.create(`${PREFIX}error-${Date.now()}`, {
      type: "basic", iconUrl: "icon128.png", title: "无法发送给 Aurum",
      message: String(error.message || error),
    });
  }
});

chrome.downloads.onCreated.addListener(async (item) => {
  if (!/^https?:\/\//i.test(item.url || "") || item.byExtensionId === chrome.runtime.id) return;
  const choice = await mode();
  if (choice === "browser") return;
  if (choice === "always") {
    try {
      await chrome.downloads.pause(item.id);
      await native({ type: "enqueue", url: item.url });
      await chrome.downloads.cancel(item.id);
    } catch (error) {
      try { await chrome.downloads.resume(item.id); } catch { /* Already completed. */ }
      chrome.notifications.create(`${PREFIX}error-${Date.now()}`, {
        type: "basic", iconUrl: "icon128.png", title: "Aurum 未能接收下载",
        message: String(error.message || error),
      });
    }
    return;
  }
  try {
    await chrome.downloads.pause(item.id);
    await chrome.storage.local.set({ [`pending-${item.id}`]: { url: item.url } });
    await chrome.alarms.create(`resume-${item.id}`, { delayInMinutes: 1 });
    await chrome.notifications.create(`${PREFIX}${item.id}`, {
      type: "basic", iconUrl: "icon128.png", title: "选择下载方式",
      message: item.filename || item.url,
      buttons: [{ title: "交给 Aurum" }, { title: `继续使用 ${browserName}` }],
      requireInteraction: true,
    });
  } catch { try { await chrome.downloads.resume(item.id); } catch { /* Already completed. */ } }
});

chrome.notifications.onButtonClicked.addListener((notificationId, button) => {
  if (!notificationId.startsWith(PREFIX)) return;
  const id = Number(notificationId.slice(PREFIX.length));
  if (Number.isInteger(id)) void finish(id, button === 0);
  void chrome.notifications.clear(notificationId);
});
chrome.notifications.onClosed.addListener((notificationId) => {
  if (!notificationId.startsWith(PREFIX)) return;
  const id = Number(notificationId.slice(PREFIX.length));
  if (Number.isInteger(id)) void finish(id, false);
});
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name.startsWith("resume-")) {
    const id = Number(alarm.name.slice(7));
    if (Number.isInteger(id)) void finish(id, false);
  }
});
