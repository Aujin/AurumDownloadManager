const host = "com.aujin.aurumdownloadmanager";
const status = document.querySelector("#status");
function native(message) {
  return new Promise((resolve, reject) => chrome.runtime.sendNativeMessage(host, message, (answer) => {
    if (chrome.runtime.lastError) reject(new Error(chrome.runtime.lastError.message));
    else if (!answer?.ok) reject(new Error(answer?.error || "连接失败"));
    else resolve(answer);
  }));
}
async function init() {
  const saved = await chrome.storage.local.get("mode");
  try {
    const answer = await native({ type: "settings" });
    document.querySelector(`input[value="${saved.mode || answer.mode}"]`).checked = true;
    status.textContent = "桌面连接可用；链接会在打开下载器后开始。";
  } catch {
    document.querySelector(`input[value="${saved.mode || "browser"}"]`).checked = true;
    status.textContent = "尚未连接桌面程序；浏览器下载会继续保留。";
  }
}
document.querySelectorAll('input[name="mode"]').forEach((input) => input.addEventListener("change", async () => {
  await chrome.storage.local.set({ mode: input.value });
  status.textContent = "已保存。";
}));
void init();
