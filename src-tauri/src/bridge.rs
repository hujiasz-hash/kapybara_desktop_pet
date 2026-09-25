//! 注入到各 WebView 的桥接脚本
//!
//! v5.0：pet（桌宠 + 悬停触发用量面板）/ config（订阅配置）/ usage（悬浮用量面板）
//! 原问答桥接（ask/answer-chunk 等）随问答功能一并移除。

/// 桌宠窗口的 window.buddy shim（pet.html 依赖面）
const PET_SHIM: &str = r#"(function () {
  function whenTauri(cb) {
    if (window.__TAURI__) return cb(window.__TAURI__);
    var t = setInterval(function () {
      if (window.__TAURI__) { clearInterval(t); cb(window.__TAURI__); }
    }, 5);
  }
  function listenEv(name, cb) {
    whenTauri(function (t) { t.event.listen(name, function (e) { cb(e.payload); }); });
  }
  function invokeLater(name, args) {
    return new Promise(function (resolve, reject) {
      whenTauri(function (t) { t.core.invoke(name, args).then(resolve, reject); });
    });
  }
  window.buddy = {
    petDragStart: function (sx, sy) { invokeLater('pet_drag_start', { sx: sx, sy: sy }); },
    petDragMove: function (sx, sy) { invokeLater('pet_drag_move', { sx: sx, sy: sy }); },
    petDragEnd: function () { invokeLater('pet_drag_end', {}); },
    petClick: function () { invokeLater('pet_click', {}); },
    hoverUsage: function (show) { invokeLater('usage_hover', { show: !!show }); },
    onCursor: function (cb) { listenEv('cursor', cb); },
    onPetEvent: function (cb) { listenEv('pet-event', function (p) { cb(p.e, p.ms, p.fallback, p.activeCount); }); },
  };
})();"#;

/// 用量类窗口（config / usage）共用的 shim
const PANEL_SHIM: &str = r#"(function () {
  function whenTauri(cb) {
    if (window.__TAURI__) return cb(window.__TAURI__);
    var t = setInterval(function () {
      if (window.__TAURI__) { clearInterval(t); cb(window.__TAURI__); }
    }, 5);
  }
  function listenEv(name, cb) {
    whenTauri(function (t) { t.event.listen(name, function (e) { cb(e.payload); }); });
  }
  function invokeLater(name, args) {
    return new Promise(function (resolve, reject) {
      whenTauri(function (t) { t.core.invoke(name, args).then(resolve, reject); });
    });
  }
  window.buddy = {
    getState: function () { return invokeLater('usage_state', {}); },
    save: function (config) { return invokeLater('usage_save', { config: config }); },
    refresh: function () { invokeLater('usage_refresh', {}); },
    hide: function () { invokeLater('hide_config', {}); },
    panelHover: function (hovered) { invokeLater('usage_panel_hover', { hovered: !!hovered }); },
    onUsageUpdate: function (cb) { listenEv('usage-update', cb); },
  };
})();"#;

/// 桌宠窗口：console 转发到主进程（对应原 webContents.on('console-message') → pet-debug.log）
const PET_CONSOLE: &str = r#"(function () {
  function petLog(level, msg) {
    try {
      if (window.__TAURI__) window.__TAURI__.core.invoke('pet_log', { level: level, message: msg });
    } catch (_) {}
  }
  ['log', 'info', 'warn', 'error'].forEach(function (lvl) {
    var orig = console[lvl].bind(console);
    console[lvl] = function () {
      var parts = Array.prototype.slice.call(arguments).map(function (x) { return String(x); });
      petLog(lvl, parts.join(' '));
      orig.apply(null, arguments);
    };
  });
  window.__kbPetLog = petLog;
})();"#;

/// KB_PET_DEBUG：周期上报 __petState()（替代原 executeJavaScript 轮询）
const PET_DEBUG_STATE: &str = r#"(function () {
  setInterval(function () {
    try {
      if (window.__petState && window.__TAURI__) {
        window.__TAURI__.core.invoke('pet_log', { level: 'info', message: '[pet-state] ' + window.__petState() });
      }
    } catch (_) {}
  }, 300);
})();"#;

/// KB_PET_DEBUG：鼠标事件探针（临时诊断触控板右键问题用，capture 阶段捕获全部）
const PET_EVENT_TRACE: &str = r#"(function () {
  ['mousedown','mouseup','contextmenu','pointerdown','pointerup'].forEach(function (t) {
    document.addEventListener(t, function (e) {
      if (window.__kbPetLog) window.__kbPetLog('info', '[pet-ev] ' + t + ' btn=' + e.button + ' buttons=' + e.buttons);
    }, true);
  });
})();"#;

fn pet_init_js() -> String {
    let mut js = String::new();
    js.push_str(PET_SHIM);
    js.push_str(PET_CONSOLE);
    // 探针：确认 shim 在 pet 窗口生效（正常路径 pet.html 无 console 输出，无法从日志判断）
    js.push_str("(function(){ function p(){ if(window.__TAURI__){ window.__TAURI__.core.invoke('pet_log',{level:'info',message:'[pet] shim-ready kbFast='+!!window.__KB_FAST}).catch(function(){}); } else { setTimeout(p,50); } } p(); })();");
    if std::env::var("KB_PET_DEBUG").is_ok() {
        js.push_str(PET_DEBUG_STATE);
        js.push_str(PET_EVENT_TRACE);
    }
    if std::env::var("KB_FAST").is_ok() {
        js.push_str("window.__KB_FAST = true;");
    }
    js
}

pub fn build_pet() -> String {
    pet_init_js()
}

pub fn build_panel() -> String {
    PANEL_SHIM.to_string()
}
