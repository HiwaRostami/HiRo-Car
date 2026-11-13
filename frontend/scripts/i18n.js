const STORAGE_KEY = "hiroLanguage";
const TIPS = {
  "connect-settings": "tipConnect",
  vin: "tipVin",
  dashboard: "tipDashboard",
  "view-dtcs": "tipDtcs",
  "readiness-tests": "tipReadiness",
  edit: "tipPids",
  graphs: "tipGraphs",
  terminal: "tipTerminal",
  settings: "tipSettings",
};

const TEXT = [
  ["#freeze-frame-status label", "freezeActive"],
  ["#demo-status label", "demoActive"],
  ["#connect-header", "connectionSettings"],
  ["#protocol-dropdown > label", "protocol"],
  ["#protocol-menu li[data-value='0']", "automatic"],
  ["#serial-port-dropdown .label-row > label", "serialPort"],
  ["#baud-rate-dropdown > label", "baudRate"],
  ["#btn-connect", "connect"],
  ["#btn-disconnect", "disconnect"],
  ["#connect-settings .text-hint", "connectHint"],
  ["#dashboard-header", "dashboard"],
  ["#dashboard > .connection-details", "dashboardHint"],
  ["#obd-clear", "clear"],
  ["#btn-save-ecu", "saveProfile"],
  [".home-label[data-i18n='homeDashboard']", "homeDashboard"],
  [".home-label[data-i18n='homeLive']", "homeLive"],
  [".home-label[data-i18n='homeFreeze']", "homeFreeze"],
  [".home-label[data-i18n='homeDiagnostics']", "homeDiagnostics"],
  [".home-label[data-i18n='homeCar']", "homeCar"],
  [".home-label[data-i18n='homeSensors']", "homeSensors"],
  [".home-label[data-i18n='homeTerminal']", "homeTerminal"],
  [".home-label[data-i18n='homeEmission']", "homeEmission"],
  ["#view-dtcs > .connection-details", "dtcsHint"],
  ["#dtc-scan-button", "scan"],
  ["#dtc-clear-button > span", "clear"],
  ["#dtc-log-file", "export"],
  ["#edit > .connection-details", "pidsHint"],
  ["#pid-add", "addPid"],
  ["#readiness-tests-header", "readiness"],
  ["#readiness-tests > .connection-details", "readinessHint"],
  ["#readiness-test-refresh", "refresh"],
  ["#readiness-test-export", "export"],
  ["#graphs-header", "graphs"],
  ["#graphs > .connection-details", "graphsHint"],
  ["#vin-decoding-header", "vinDecoding"],
  ["#vin > .connection-details", "vinHint"],
  ["#vin-export", "export"],
  ["#vin-decode-start", "decode"],
  ["#terminal > .screen-label", "terminal"],
  ["#terminal > .connection-details", "terminalHint"],
  ["#settings-header", "settings"],
  ["#settings > .connection-details", "settingsHint"],
  ["[data-i18n='logging']", "logging"],
  ["[data-i18n='recordResponses']", "recordResponses"],
  ["[data-i18n='replayResponses']", "replayResponses"],
  ["[data-i18n='hideNotifications']", "hideNotifications"],
  ["#log-file-button", "chooseFile"],
  ["[data-i18n='logFilePath']", "logFilePath"],
  ["[data-i18n='diagnostics']", "diagnostics"],
  ["[data-i18n='saveDtcs']", "saveDtcs"],
  ["[data-i18n='autoCheckCodes']", "autoCheckCodes"],
  ["[data-i18n='freezeFrame']", "freezeFrame"],
  ["[data-i18n='autoConnect']", "autoConnect"],
  ["[data-i18n='privacy']", "privacy"],
  ["[data-i18n='partialVin']", "partialVin"],
  ["[data-i18n='deleteLogs']", "deleteLogs"],
  ["[data-i18n='units']", "units"],
  ["[data-i18n='speed']", "speed"],
  ["[data-i18n='distance']", "distance"],
  ["[data-i18n='temperature']", "temperature"],
  ["[data-i18n='torque']", "torque"],
  ["[data-i18n='pressure']", "pressure"],
  ["[data-i18n='flowRate']", "flowRate"],
  ["#app-language-label", "appLanguage"],
];

window.hiroLang = "en";
window.hiroI18n = {};
window.hiroPlatform = /Android/i.test(navigator.userAgent) ? "android" : "desktop";

export function paintHomeLink() {
  const button = document.getElementById("home-connect");
  const label = document.getElementById("home-status-label");
  if (button) button.textContent = window.connected ? t("disconnect") : t("connect");
  if (label) {
    const chip = document.getElementById("connection-label");
    label.textContent = window.connected && chip?.textContent
      ? chip.textContent
      : t("notConnected");
  }
}

export function t(key, vars = {}) {
  let value = window.hiroI18n?.[key];
  if (typeof value !== "string") value = key;
  return value.replace(/\{(\w+)\}/g, (_, name) =>
    vars[name] === undefined ? "" : String(vars[name]),
  );
}

export function metricLabel(name) {
  return window.hiroI18n?.metrics?.[name] || name;
}

function chartFont() {
  return document.documentElement.dir === "rtl" ? "IRANSans" : "Segoe UI, sans-serif";
}

function applyStatic() {
  for (const [selector, key] of TEXT) {
    document.querySelectorAll(selector).forEach((el) => {
      el.textContent = t(key);
    });
  }

  document.querySelectorAll(".sidebar-button[data-screen]").forEach((btn) => {
    const key = TIPS[btn.dataset.screen];
    if (key) btn.dataset.tooltip = t(key);
  });

  const search = document.querySelector(".search");
  if (search) search.placeholder = t("search");
  const vinInput = document.querySelector(".vin-input");
  if (vinInput) vinInput.placeholder = t("enterVin");

  document.querySelectorAll(".graph-dropdown-toggle").forEach((btn) => {
    if (!btn.dataset.metricKey) btn.textContent = t("noneSelected");
    else btn.textContent = metricLabel(btn.dataset.metricKey);
  });
  document.querySelectorAll("li[data-metric-key]").forEach((li) => {
    li.textContent = metricLabel(li.dataset.metricKey);
  });
  document.querySelectorAll("h3[data-metric-key]").forEach((h3) => {
    h3.textContent = metricLabel(h3.dataset.metricKey);
  });

  const pause = document.getElementById("obd-pause");
  if (pause) pause.textContent = window.obdViewPaused ? t("resume") : t("pause");

  const demoOption = document.querySelector("#serial-port-dropdown-menu li[data-value='DEMO MODE']");
  if (demoOption) demoOption.textContent = t("demoMode");
  const protocolSelected = document.getElementById("protocol-selected");
  if (protocolSelected?.dataset.value === "0") protocolSelected.textContent = t("automatic");
  const selectedPort = document.getElementById("serial-port-selected");
  if (selectedPort && !window.connected) {
    const value = (selectedPort.dataset.value || selectedPort.textContent || "").trim();
    if (value === "DEMO MODE" || value === "Demo" || value === "نمایشی") {
      selectedPort.dataset.value = "DEMO MODE";
      selectedPort.textContent = t("demoMode");
    }
  }

  document.body.classList.toggle("is-connected", Boolean(window.connected));
  document.body.classList.toggle(
    "is-demo",
    Boolean(window.connected) && window.connectionConfig?.serialPort === "DEMO MODE",
  );

  if (!window.connected) {
    const label = document.getElementById("connection-label");
    const status = document.getElementById("connection-details");
    const car = document.querySelector(".car-model");
    if (label) label.textContent = t("notConnected");
    if (status) status.textContent = t("noConnection");
    if (car && !car.dataset.filled) car.textContent = t("unavailable");
    const vin = document.querySelector(".vin");
    if (vin && !vin.dataset.filled) {
      vin.innerHTML = `${t("vinPrefix")} <span class="obd-ltr">—</span>`;
    }
  } else if (window.connectionConfig?.serialPort === "DEMO MODE") {
    const label = document.getElementById("connection-label");
    const status = document.getElementById("connection-details");
    if (label) label.textContent = t("connectedDemo");
    if (status) status.textContent = t("demoConnected");
  } else if (window.connectionConfig?.serialPort) {
    const label = document.getElementById("connection-label");
    if (label) {
      label.textContent = t("connectedVia", {
        port: String(window.connectionConfig.serialPort).toUpperCase(),
      });
    }
  }

  paintHomeLink();

  const note = document.getElementById("android-connect-note");
  if (note) {
    note.textContent = t("androidBluetoothHint");
    note.style.display = window.hiroPlatform === "android" ? "flex" : "none";
  }
  const vinNote = document.getElementById("vin-db-status");
  if (vinNote && window.hiroVinDatabaseIncluded === false) {
    vinNote.textContent = t("vinDbExcluded");
    vinNote.style.display = "block";
  }

  document.title = t("appTitle");
  const langButton = document.getElementById("language-selected");
  if (langButton) {
    langButton.dataset.value = window.hiroLang;
    langButton.textContent = window.hiroLang === "fa" ? "فارسی" : "English";
  }

  Object.values(window.charts || {}).forEach((chart) => {
    if (!chart?.options?.scales) return;
    const font = chartFont();
    for (const axis of ["x", "y"]) {
      chart.options.scales[axis].ticks.font.family = font;
      chart.options.scales[axis].title.font.family = font;
    }
    if (chart.options.scales.x.title.text) {
      chart.options.scales.x.title.text = t("chartTime");
    }
    chart.update();
  });
}

export async function applyLanguage(lang, persist) {
  const next = lang === "fa" ? "fa" : "en";
  const response = await fetch(`./locales/${next}.json`);
  window.hiroI18n = await response.json();
  window.hiroLang = next;
  document.documentElement.lang = next;
  document.documentElement.dir = next === "fa" ? "rtl" : "ltr";
  if (persist) localStorage.setItem(STORAGE_KEY, next);
  applyStatic();
  window.dispatchEvent(new CustomEvent("hiro-language-changed"));
}

export async function initI18n() {
  const saved = localStorage.getItem(STORAGE_KEY);
  const lang = saved === "fa" || saved === "en" ? saved : "fa";
  await applyLanguage(lang, false);
}

const THEME_KEY = "hiroTheme";

function applyTheme(theme) {
  const next = theme === "dark" ? "dark" : "light";
  document.documentElement.dataset.theme = next;
  localStorage.setItem(THEME_KEY, next);
}

document.getElementById("theme-toggle")?.addEventListener("click", () => {
  applyTheme(document.documentElement.dataset.theme === "dark" ? "light" : "dark");
});

await initI18n();

document.getElementById("language-menu")?.addEventListener("click", (event) => {
  const item = event.target.closest("li");
  if (!item) return;
  applyLanguage(item.dataset.value, true);
});
