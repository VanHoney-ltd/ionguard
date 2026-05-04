import './style.css';
import './app.css';

import {
  StartMonitor,
  StopMonitor,
  IsMonitoring,
  TriggerSweep,
  SetInterval,
  IgnoreDevices,
  GetDevices,
  GetEvents,
  GetStatus,
} from '../wailsjs/go/main/App';
import { EventsOn } from '../wailsjs/runtime/runtime';

// State
let monitoring = false;
let devices = {};
let events = [];
let deviceCount = 0;

// DOM
const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => document.querySelectorAll(sel);

function formatTime(ts) {
  const d = new Date(ts * 1000);
  return d.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

function render() {
  renderHeader();
  renderDevices();
  renderEvents();
}

function renderHeader() {
  const pill = $('#status-pill');
  if (!pill) return;
  if (monitoring) {
    pill.classList.add('active');
    pill.innerHTML = '<span class="status-dot"></span> Monitoring';
  } else {
    pill.classList.remove('active');
    pill.innerHTML = '<span class="status-dot"></span> Standby';
  }

  const btn = $('#toggle-btn');
  if (btn) {
    btn.textContent = monitoring ? '⏹ Stop' : '▶ Start';
    btn.className = monitoring ? 'btn danger' : 'btn primary';
  }

  const devCount = $('#dev-count');
  if (devCount) devCount.textContent = deviceCount;
}

function renderDevices() {
  const container = $('#device-list');
  if (!container) return;

  const items = Object.values(devices).sort((a, b) => a.ip.localeCompare(b.ip));
  if (items.length === 0) {
    container.innerHTML = `
      <div class="empty">
        <div class="empty-icon">📡</div>
        No devices detected yet
      </div>`;
    return;
  }

  container.innerHTML = items.map(dev => `
    <div class="device-item" data-ip="${dev.ip}">
      <div class="device-icon">🖧</div>
      <div class="device-info">
        <div class="device-ip">${dev.ip}</div>
        <div class="device-mac">${dev.mac || '??'} &middot; ${dev.iface || '?'}</div>
      </div>
      <div class="device-meta">
        <div class="device-state ${dev.state || 'unknown'}">${dev.state || 'unknown'}</div>
      </div>
    </div>
  `).join('');
}

function renderEvents() {
  const container = $('#event-log');
  if (!container) return;

  const evts = [...events].reverse().slice(0, 100);
  if (evts.length === 0) {
    container.innerHTML = `
      <div class="empty">
        <div class="empty-icon">📋</div>
        No events yet
      </div>`;
    return;
  }

  container.innerHTML = evts.map(evt => {
    let msg = '';
    if (evt.event_type === 'new_device') {
      msg = `${evt.ip} joined (${evt.mac || '?'})`;
    } else if (evt.event_type === 'device_gone') {
      msg = `${evt.ip} left`;
    } else if (evt.event_type === 'mac_change') {
      msg = `${evt.ip} MAC changed: ${evt.old_mac || '?'} → ${evt.new_mac || '?'}`;
    } else {
      msg = `${evt.ip}: ${evt.event_type}`;
    }
    return `
      <div class="event-item ${evt.event_type}">
        <span class="event-time">${formatTime(evt.timestamp)}</span>
        <span class="event-type">${evt.event_type.replace('_', ' ')}</span>
        <span class="event-msg">${msg}</span>
      </div>
    `;
  }).join('');
}

// Actions
window.toggleMonitor = async function () {
  try {
    if (monitoring) {
      await StopMonitor();
    } else {
      await StartMonitor();
    }
    monitoring = await IsMonitoring();
    render();
  } catch (e) {
    console.error(e);
  }
};

window.triggerSweep = async function () {
  try {
    await TriggerSweep();
  } catch (e) {
    console.error(e);
  }
};

window.setInterval = async function () {
  const val = parseInt($('#interval-input').value, 10);
  if (!isNaN(val) && val >= 1) {
    try {
      await SetInterval(val * 1000);
    } catch (e) {
      console.error(e);
    }
  }
};

window.ignoreDevices = async function () {
  const val = $('#ignore-input').value.trim();
  if (val) {
    try {
      await IgnoreDevices(val);
      $('#ignore-input').value = '';
    } catch (e) {
      console.error(e);
    }
  }
};

// Build DOM
function initDOM() {
  $('#app').innerHTML = `
    <div class="header">
      <div class="brand">
        <div class="brand-icon">iG</div>
        <div>
          <div class="brand-title">ionguard</div>
          <div class="brand-sub">NEMESIS ENGINE LAN Monitor</div>
        </div>
      </div>
      <div class="status-pill" id="status-pill">
        <span class="status-dot"></span> Standby
      </div>
    </div>

    <div class="controls">
      <button class="btn primary" id="toggle-btn" onclick="toggleMonitor()">▶ Start</button>
      <button class="btn" onclick="triggerSweep()">🔍 Sweep</button>
      <input class="input" id="interval-input" type="number" placeholder="Interval (s)" min="1" value="3" style="width: 90px;" onchange="setInterval()" />
      <input class="input" id="ignore-input" type="text" placeholder="Ignore IPs (comma-sep)" style="width: 160px;" onchange="ignoreDevices()" />
    </div>

    <div class="main">
      <div class="panel" style="flex: 1;">
        <div class="panel-header">
          <span>Devices</span>
          <span class="count" id="dev-count">0</span>
        </div>
        <div class="panel-body">
          <div class="device-list" id="device-list"></div>
        </div>
      </div>

      <div class="panel" style="flex: 1; min-width: 320px;">
        <div class="panel-header">
          <span>Event Log</span>
        </div>
        <div class="panel-body">
          <div class="event-log" id="event-log"></div>
        </div>
      </div>
    </div>
  `;
}

// Event bindings
EventsOn('ionguard:event', (data) => {
  events.push(data);
  if (events.length > 500) events.shift();

  // Update devices map from events
  if (data.event_type === 'new_device') {
    devices[data.ip] = {
      ip: data.ip,
      mac: data.mac || '?',
      iface: '-',
      state: 'reachable',
      last_seen: data.timestamp,
    };
  } else if (data.event_type === 'device_gone') {
    delete devices[data.ip];
  } else if (data.event_type === 'mac_change') {
    if (devices[data.ip]) {
      devices[data.ip].mac = data.new_mac || devices[data.ip].mac;
    }
  }

  deviceCount = Object.keys(devices).length;
  render();
});

EventsOn('ionguard:status', (data) => {
  monitoring = data.running;
  deviceCount = data.devices;
  render();
});

// Init
initDOM();

// Load initial state
(async () => {
  try {
    monitoring = await IsMonitoring();
    const devs = await GetDevices();
    devices = devs || {};
    deviceCount = Object.keys(devices).length;
    const evts = await GetEvents();
    events = evts || [];
    render();
    await GetStatus();
  } catch (e) {
    console.error('Init error:', e);
  }
})();
