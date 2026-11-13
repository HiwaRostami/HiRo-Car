const { invoke } = window.__TAURI__.core;
const picker = document.createElement("dialog");
picker.className = "connection-picker";
const title = document.createElement("h3");
const list = document.createElement("div");
const close = document.createElement("button");
close.type = "button";
close.style.cssText = "margin-top:20px;padding:10px 20px";
close.addEventListener("click", () => picker.close());
picker.append(title, list, close);
document.body.appendChild(picker);
let responseTimer;

const isPersian = () => document.documentElement.lang === "fa";
function showDevices(payload) {
  if (!picker.open) return;
  clearTimeout(responseTimer);
  list.replaceChildren();
  const devices = (payload || []).filter(([name]) => name !== "DEMO MODE");
  if (!devices.length) {
    list.textContent = isPersian()
      ? "دستگاه جفت‌شده‌ای پیدا نشد. بلوتوث و مجوز دستگاه‌های نزدیک را بررسی کنید و دانگل را در تنظیمات گوشی جفت کنید."
      : "No paired devices found. Check Bluetooth and Nearby devices permission, and pair the adapter in phone settings.";
  }
  for (const [name, baud] of devices) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = name;
    button.dir = "ltr";
    button.style.cssText = "display:block;width:100%;padding:16px;margin:8px 0;text-align:start";
    button.addEventListener("click", () => {
      const selected = document.getElementById("serial-port-selected");
      selected.textContent = name;
      selected.dataset.value = name;
      document.getElementById("baud-rate-selected").textContent = String(baud);
      picker.close();
      document.querySelectorAll(".screen").forEach((screen) => {
        screen.style.display = screen.id === "connect-settings" ? "flex" : "none";
      });
      document.body.classList.remove("is-home");
    });
    list.appendChild(button);
  }
}

document.getElementById("home-connect").addEventListener("click", async (event) => {
  if (window.connected) return;
  event.stopImmediatePropagation();
  title.textContent = isPersian() ? "انتخاب دستگاه جفت‌شده" : "Select a paired device";
  close.textContent = isPersian() ? "بستن" : "Close";
  list.textContent = isPersian() ? "در حال دریافت دستگاه‌ها…" : "Loading devices…";
  picker.dir = document.documentElement.dir || "ltr";
  if (!picker.open) picker.showModal();
  clearTimeout(responseTimer);
  responseTimer = setTimeout(() => {
    if (picker.open) list.textContent = isPersian()
      ? "پاسخی برای درخواست فهرست دریافت نشد. دوباره تلاش کنید."
      : "The device list request timed out. Please retry.";
  }, 7000);
  try {
    showDevices(await invoke("paired_devices"));
  } catch (error) {
    clearTimeout(responseTimer);
    if (!picker.open) return;
    list.textContent = String(error) === "BLUETOOTH_DISABLED"
      ? (isPersian()
        ? "\u0628\u0644\u0648\u062a\u0648\u062b \u062e\u0627\u0645\u0648\u0634 \u0627\u0633\u062a. \u0622\u0646 \u0631\u0627 \u0631\u0648\u0634\u0646 \u06a9\u0646\u06cc\u062f \u0648 \u062f\u0648\u0628\u0627\u0631\u0647 \u062a\u0644\u0627\u0634 \u06a9\u0646\u06cc\u062f."
        : "Bluetooth is off. Turn it on and try again.")
      : String(error);
  }
}, { capture: true });
