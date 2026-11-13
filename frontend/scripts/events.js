const { listen, emit } = window.__TAURI__.event;
import {
  exportDtcs,
  addGraphDropdownOption,
  appendTerminalOutput,
  addNotification,
  listenExpandPID,
} from "./features.js";
import {
  saveConnectionConfig,
  updateUnitConversionDropdowns,
} from "./settings.js";
import { t, metricLabel, paintHomeLink } from "./i18n.js";

const connectionLabel = document.getElementById("connection-label");
const connectionIcon = document.getElementById("connection-icon");
const replayResponses = document.getElementById("replay-responses");
const recordResponses = document.getElementById("record-responses");
const connectButton = document.getElementById("btn-connect");
const disconnectButton = document.getElementById("btn-disconnect");
const demoStatus = document.getElementById("demo-status");

function odometerModel(raw) {
  const text = String(raw ?? "").trim();
  if (text === "" || text.toUpperCase() === "N/A") {
    return { digits: ["-", "-", "-"], dimUntil: 0, decimalAt: -1 };
  }

  const numeric = Number(text);
  if (!Number.isFinite(numeric)) return { plain: text };

  const negative = text.startsWith("-");
  const absText = text.replace(/^[+-]/, "");
  const decimals = absText.includes(".")
    ? Math.min(1, (absText.split(".")[1] || "").length)
    : 0;
  const [intPart, fracPart = ""] = Math.abs(numeric).toFixed(decimals).split(".");
  const width = Math.min(6, Math.max(3, intPart.length));
  const shownInt =
    intPart.length > width ? intPart.slice(-width) : intPart.padStart(width, "0");
  const digits = shownInt.split("");
  if (negative) digits.unshift("-");

  const intCount = digits.length;
  let dimUntil = digits[0] === "-" ? 1 : 0;
  for (let index = dimUntil; index < intCount; index += 1) {
    if (digits[index] !== "0") break;
    dimUntil = index + 1;
  }
  const full = fracPart ? digits.concat(fracPart.split("")) : digits;
  if (!fracPart && dimUntil > full.length - 1) dimUntil = full.length - 1;

  return {
    digits: full,
    dimUntil,
    decimalAt: fracPart ? intCount : -1,
  };
}

function paintOdometer(valueElem, rawValue, unit) {
  const noData = String(unit || "").toLowerCase() === "no data";
  const model = noData ? odometerModel("N/A") : odometerModel(rawValue);
  const windowEl = document.createElement("span");
  windowEl.className = "odo-window";
  if (model.plain) {
    const plain = document.createElement("span");
    plain.className = "odo-plain";
    plain.textContent = model.plain;
    windowEl.appendChild(plain);
  } else {
    if (model.digits.length > 5) windowEl.classList.add("is-wide");
    model.digits.forEach((digit, index) => {
      if (model.decimalAt === index) {
        const dot = document.createElement("span");
        dot.className = "odo-sep";
        windowEl.appendChild(dot);
      }
      const cell = document.createElement("span");
      cell.className = "odo-digit" + (index < model.dimUntil ? " is-dim" : "");
      cell.textContent = digit;
      windowEl.appendChild(cell);
    });
  }

  const unitSpan = document.createElement("span");
  unitSpan.className = "unit obd-ltr";
  unitSpan.textContent = noData ? "" : unit || "";
  valueElem.classList.add("odo");
  valueElem.replaceChildren(windowEl, unitSpan);
}

listen("update-card", (event) => {
  const cards = document.querySelectorAll("#dashboard-cards .card");
  const exists = Array.from(cards).some((card) => {
    return card.querySelector("h3")?.dataset.metricKey === event.payload.name;
  });

  if (window.obdViewPaused) return;

  // create card if it doesnt exist
  if (!exists) {
    const container = document.querySelector(".grid");
    if (!container) return;

    const card = document.createElement("div");
    const h3 = document.createElement("h3");
    const valueDiv = document.createElement("div");

    card.className = "card";
    valueDiv.className = "value odo";
    h3.dataset.metricKey = event.payload.name;
    h3.textContent = metricLabel(event.payload.name);
    paintOdometer(valueDiv, event.payload.value, event.payload.unit);

    card.appendChild(h3);
    card.appendChild(valueDiv);
    container.appendChild(card);
  } else {
  cards.forEach((card) => {
    if (card.classList.contains("dimmed")) return;

    const h3 = card.querySelector("h3");
    if (!h3) return;

    const title = h3.dataset.metricKey || h3.textContent;
    if (title == event.payload.name) {
      const valueElem = card.querySelector(".value");
      if (valueElem) paintOdometer(valueElem, event.payload.value, event.payload.unit);
    }
  });
  }

  // update any graphs that might be using this
  emit("update-graphs", {
    name: event.payload.name,
    value: event.payload.value,
    unit: event.payload.unit,
  });
});

listen("vehicle-details", (event) => {
  const vin = document.querySelector(".vin");
  const makeModel = document.querySelector(".car-model");

  const vinValue = !hideVin
    ? event.payload.vin.toUpperCase()
    : event.payload.vin.toUpperCase().slice(0, 6) + "*".repeat(12);
  vin.dataset.filled = "1";
  vin.innerHTML = `${t("vinPrefix")} <span class="obd-ltr">${vinValue}</span>`;

  makeModel.dataset.filled = "1";
  makeModel.textContent = `${event.payload.make} ${event.payload.model}`.trim();
});

listen("connection-status", async (event) => {
  const protocolDropdown = document.getElementById("protocol-menu");
  const protocolSelected = document.getElementById("protocol-selected");
  const status = document.getElementById("connection-details");

  if (event.payload.connected) {
    if (window.connected) return;

    const serialPort = event.payload.serialPort;
    const baudRate = event.payload.baudRate;
    const protocol = event.payload.protocol;

    const isDemo = serialPort === "DEMO MODE";
    connectionLabel.textContent = isDemo
      ? t("connectedDemo")
      : t("connectedVia", { port: serialPort.toUpperCase() });
    status.textContent = isDemo
      ? t("demoConnected")
      : t("connectedDetail", { port: serialPort.toUpperCase() });
    connectionIcon.src = "/assets/icons/connected.png";
    window.connected = true;
    document.body.classList.add("is-connected");
    document.body.classList.toggle("is-demo", isDemo);

    // Features only accessible in real mode
    if (serialPort !== "DEMO MODE") {
      recordResponses.disabled = false;
      replayResponses.disabled = false;
    } else
      // show demo mode status
      demoStatus.style.display = "flex";

    connectButton.disabled = true;
    disconnectButton.disabled = false;
    window.connectPending = false;
    paintHomeLink();

    document.getElementById("baud-rate-selected").textContent = baudRate;
    document.getElementById("serial-port-selected").textContent = serialPort;
    protocolSelected.dataset.value = protocol;
    protocolSelected.textContent = protocolDropdown.querySelector(
      `li[data-value="${protocol}"]`,
    ).textContent;

    window.connectionConfig = {
      serialPort: serialPort,
      baudRate: baudRate,
      protocol: parseInt(protocol),
    };

    addNotification(
      t("connectedTitle"),
      serialPort === "DEMO MODE" ? t("demoConnected") : event.payload.message,
    );

    saveConnectionConfig();

    if (window.autoCheckCodes) {
      emit("get-dtcs");
    }

    if (window.unitPreferences) {
      emit("set-unit-preferences", window.unitPreferences);
      updateUnitConversionDropdowns();
    }

    if (serialPort === "DEMO MODE") {
      await new Promise((r) => setTimeout(r, 15000));
      emit("get-pids");
      await new Promise((r) => setTimeout(r, 2000));
      emit("get-readiness-tests");
    } else {
      await new Promise((r) => setTimeout(r, 7000));
      emit("get-pids");
      await new Promise((r) => setTimeout(r, 2000));
      emit("get-readiness-tests");
    }
  } else if (event.payload.message === "ANDROID_UNSUPPORTED") {
    connectButton.disabled = false;
    disconnectButton.disabled = true;
    window.connected = false;
    window.connectPending = false;
    document.body.classList.remove("is-connected", "is-demo");
    paintHomeLink();
    addNotification(t("notice"), t("androidUnsupported"));
    return;
  } else if (!window.connected) {
    connectButton.disabled = false;
    disconnectButton.disabled = true;
    const failed = window.connectPending;
    window.connectPending = false;
    paintHomeLink();
    if (failed) {
      const detail = event.payload.message || t("connectFailed");
      status.textContent = detail;
      addNotification(t("notice"), t("connectFailed"));
    }
    return;
  } else {
    connectionLabel.textContent = t("notConnected");
    status.textContent = t("noConnection");
    connectionIcon.src = "/assets/icons/not-connected.png";
    window.connected = false;
    document.body.classList.remove("is-connected", "is-demo");
    recordResponses.disabled = true;
    replayResponses.disabled = true;
    connectButton.disabled = false;
    disconnectButton.disabled = true;
    window.connectPending = false;
    paintHomeLink();

    addNotification(t("disconnectedTitle"), event.payload.message);

    if (demoStatus.style.display !== "none") demoStatus.style.display = "none";
  }
});

listen("update-pids", (event) => {
  const pids = event.payload;
  const pidList = document.getElementById("pid-list");
  pidList.innerHTML = ``;

  for (const pidInfo of pids) {
    const pidGroup = document.createElement("div");
    pidGroup.className = "pid-group";
    pidGroup.innerHTML = `
        <div class="pid-container" ${!pidInfo.supported ? 'style="opacity: 0.15"' : ""} >
            <div class="info-row">
            <button class="arrow-icon"><img src="/assets/icons/arrow-icon.png"></button>
            <span class="pid-badge ${pidInfo.supported ? "is-on" : "is-off"}">${pidInfo.supported ? t("supported") : t("unsupported")}</span>
            <span class="name">${pidInfo.pidName}</span>
            </div>
            <div class="pid-details" style="display: none; height: 0;">
            <div class="pid-data-columns">
                <div class="pid-column">
                <div class="pid-label">${t("mode")}</div>
                <div class="pid-value">$${pidInfo.mode}</div>
                </div>
                <div class="pid-column">
                <div class="pid-label">${t("pid")}</div>
                <div class="pid-value">${pidInfo.pid}</div>
                </div>
                <div class="pid-column">
                <div class="pid-label">${t("command")}</div>
                <div class="pid-value">${pidInfo.mode + pidInfo.pid}</div>
                </div>
                <div class="pid-column">
                <div class="pid-label">${t("equation")}</div>
                <div class="pid-value">${pidInfo.formula == "" ? "??" : pidInfo.formula}</div>
                </div>
                <div class="pid-column">
                <div class="pid-label">${t("unit")}</div>
                <div class="pid-value">${pidInfo.unit == "" ? "??" : pidInfo.unit}</div>
                </div>
            </div>
            <button class="pid-button" id="remove-pid">${t("remove")}</button>
            </div>
        </div>
        `;

    pidList.appendChild(pidGroup);

    listenExpandPID(pidGroup);

    addGraphDropdownOption(
      pidInfo.pid,
      pidInfo.pidName,
      pidInfo.unit,
      pidInfo.formula,
    );
  }

  // Increment results counter
  const header = document.getElementById("pid-list-header");
  if (header.textContent != "VIEW PIDS (" + pidList.children.length + ")") {
    addNotification(t("pidIndex"), t("pidUpdated"));
  }

  header.textContent = t("viewPids", { count: pidList.children.length });
});

const dtcList = document.getElementById("dtc-list");
const dtcHeader = document.getElementById("dtc-header");

listen("update-dtcs", (event) => {
  const dtcs = event.payload;
  if (!dtcs) {
    dtcHeader.textContent = "DIAGNOSTIC TROUBLE CODES (0)";
    return;
  }

  dtcHeader.textContent = t("dtcsCount", { count: dtcs.length });

  // clear dtc list
  dtcList.innerHTML = ``;

  for (const troubleCode of dtcs) {
    const description =
      troubleCode.description +
      " [" +
      t(troubleCode.status) +
      "]";

    let dtcRow = document.createElement("div");
    dtcRow.className = "info-row dtc-row";
    dtcRow.dataset.status = troubleCode.status;
    dtcRow.style = "height: 60px; position: relative;";
    dtcRow.innerHTML = `
          <div class="category" id="dtc-category" style="font-size: 40px; font-weight: 900; margin-left: 6px; min-width: 70px; text-align: center; display: inline-block;">
              ${troubleCode.category}
          </div>
          <div class="name" style="display: inline-block; vertical-align: top;">
              <span class="name obd-ltr" id="dtc-name" style="font-size: 25px; font-weight: 700;">${troubleCode.name}</span>
              <div class="name" id="dtc-location" style="color: #BDBDBD; font-size: 15px; margin-top: -5px; font-weight: 600;">
              ${troubleCode.location}
              </div>
          </div>
          
          <svg xmlns="http://www.w3.org/2000/svg" width="40" height="30" viewBox="0 0 24 24" fill="none"
              stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"
              style="margin-left: 190px; position: absolute; top: 50%; transform: translateY(-50%);">
              <path d="M5 12h14" />
              <path d="m12 5 7 7-7 7" />
          </svg>
          
          <div class="name" id="dtc-description"
              style="position: absolute; color: #BDBDBD; left: 240px; top: 0; bottom: 0; max-width: 600px; overflow-wrap: break-word; display: flex; align-items: center; height: 100%;">
              ${description}
          </div>
          `;

    dtcList.appendChild(dtcRow);
  }

  if (window.autoSaveCodes) {
    exportDtcs(true);
  }
});

const serialPortMenu = document.getElementById("serial-port-dropdown-menu");
const serialPortSelected = document.getElementById("serial-port-selected");
const baudRateSelected = document.getElementById("baud-rate-selected");

export const serialPortListenerReady = listen("update-serial-ports", (event) => {
  serialPortMenu.innerHTML = "";

  const demoModePortOption = document.createElement("li"); 
  demoModePortOption.textContent = t("demoMode");
  demoModePortOption.dataset.value = "DEMO MODE";
  serialPortMenu.appendChild(demoModePortOption);

  demoModePortOption.addEventListener("click", () => {
    baudRateSelected.textContent = "0";
  })

  if (!event.payload) {
    return;
  }
  
  console.log("received serial ports:", event);

  for (const [port, baudRate] of event.payload) {
    // append serial port connect option
    const portOption = document.createElement("li");
    portOption.className = "obd-ltr";
    portOption.textContent = port;
    portOption.dataset.value = port;
    portOption.dataset.baud = String(baudRate);
    serialPortMenu.appendChild(portOption);

    portOption.addEventListener("click", () => {
      baudRateSelected.textContent = baudRate;
    });
  }

  const refreshButton = document.getElementById("refresh-serial-ports");
  const refreshIcon = refreshButton.querySelector("svg");

  const current = (serialPortSelected.dataset.value || serialPortSelected.textContent || "").trim();
  const stillDemo = current === "DEMO MODE" || current === t("demoMode");
  const preferred = event.payload.find(([port]) => /obd|elm|vlink|vgate/i.test(port));
  if (preferred && stillDemo && !window.connected) {
    serialPortSelected.textContent = preferred[0];
    serialPortSelected.dataset.value = preferred[0];
    baudRateSelected.textContent = String(preferred[1]);
  }

  refreshIcon.classList.remove("spinning");
});

const readinessTests = document.getElementById("readiness-tests-list");
listen("update-readiness-tests", (event) => {
  readinessTests.innerHTML = ``;

  addNotification(t("readinessTitle"), t("readinessUpdated"));

  const tests = event.payload;
  for (const test of tests) {
    const testRow = document.createElement("div");
    testRow.className = "info-row";
    testRow.dataset.available = String(test.available);
    testRow.dataset.complete = String(test.complete);
    testRow.style = "justify-content: space-between;";
    testRow.innerHTML = `
<div class="name" id="test-name" style="flex: 2; font-weight: 700; color: #f7f3ff;">${t("testPrefix")} ${test.name}</div>
    <div class="name" id="test-availability" style="flex: 1; font-size: 0.97rem; font-weight: 500; text-transform: uppercase; color: #d4dadf;">${test.available ? t("testAvailable") : t("testUnavailable")}</div>
      <div class="name" id="test-completeness" style="flex: 1; font-size: 0.97rem; font-weight: 500; text-transform: uppercase; color: #d4dadf;">${test.complete ? t("ready") : t("notReady")}</div>

    `;

    readinessTests.appendChild(testRow);
  }
});

listen("update-command-output", (event) => {
  appendTerminalOutput(event.payload);
});

const exportButton = document.getElementById("vin-export");

// show vehicle info if no error
listen("decode-vin-result", (event) => {
  const data = event.payload;
  addNotification(t("vinTitle"), data.error_msg);

  const container = document.getElementById("vin-container");
  if (!container) {
    return;
  }

  container.innerHTML = "";

  // show all fields
  for (const [key, value] of Object.entries(event.payload)) {
    if (key === "error_msg" || value == "N/A" || value == "-1") continue;

    const vinLabels = {
      vin: "vinPrefix",
      make: "make",
      model: "model",
      model_year: "modelYear",
      engine_model: "engineModel",
      cylinder_count: "cylinders",
      fuel_type: "fuelType",
      body_style: "bodyStyle",
    };
    const name = vinLabels[key]
      ? t(vinLabels[key]).replace(/:$/, "")
      : key.replaceAll("_", " ");

    const card = document.createElement("div");
    const h3 = document.createElement("h3");
    const valueDiv = document.createElement("div");

    card.className = "card";
    valueDiv.className = "value";

    h3.textContent = name;
    valueDiv.textContent = key === "vin" ? String(value).toUpperCase() : String(value);
    if (key === "vin") valueDiv.classList.add("obd-ltr");
    valueDiv.style.fontSize = "1.3rem";
    valueDiv.style.wordBreak = "break-word";
    valueDiv.style.whiteSpace = "normal";
    valueDiv.style.maxWidth = "320px";

    // append elements to card
    card.appendChild(h3);
    card.appendChild(valueDiv);

    container.appendChild(card);
  }

  exportButton.disabled = false;
});

listen("display-notification", (event) => {
  const title = event.payload.title;
  let desc = event.payload.description;
  if (desc === "VIN_DB_EXCLUDED") desc = t("vinDbExcluded");
  addNotification(title === "VIN" ? t("vinTitle") : title, desc);
});

listen("android-unsupported", () => {
  addNotification(t("notice"), t("androidUnsupported"));
  connectButton.disabled = false;
});

listen("app-info", (event) => {
  window.hiroPlatform = event.payload.platform || window.hiroPlatform;
  window.hiroVinDatabaseIncluded = event.payload.vinDatabaseIncluded;
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
});
