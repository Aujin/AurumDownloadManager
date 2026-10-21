const host = "com.aujin.aurumdownloadmanager";
const status = document.querySelector("#status");
document.querySelector("#identity").textContent = `版本 ${chrome.runtime.getManifest().version} · 插件 ID ${chrome.runtime.id}`;
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
  } catch (error) {
    document.querySelector(`input[value="${saved.mode || "browser"}"]`).checked = true;
    status.textContent = `连接失败：${error.message || error}。请运行新版 Aurum，在“设置 → 浏览器”点击“修复 Edge 连接”，再重新加载插件。`;
  }
}
document.querySelectorAll('input[name="mode"]').forEach((input) => input.addEventListener("change", async () => {
  await chrome.storage.local.set({ mode: input.value });
  status.textContent = "已保存。";
}));
void init();
