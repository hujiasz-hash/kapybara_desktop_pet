/**
 * Gemini Buddy — 快捷键呼出的桌面 Gemini 快速问答小窗
 *
 * - 呼出/隐藏: Option+G（改 TOGGLE_ACCELERATOR 可换）
 * - 窗口内直接使用 gemini.google.com，登录态持久保存在本应用 session
 * - Esc 在输入框为空时隐藏窗口
 * - 代理: 启动时设置 GB_PROXY=http://127.0.0.1:7898 可强制走代理（默认走系统代理）
 */
const { app, BrowserWindow, globalShortcut, screen, shell, Notification } = require('electron');
const fs = require('fs');

const GEMINI_URL = 'https://gemini.google.com/app';
const TOGGLE_ACCELERATOR = 'Option+G';
const WIN_WIDTH = 760;
const WIN_HEIGHT = 680;
const PARTITION = 'persist:gemini';

// 可选代理（须在 app ready 前设置）
if (process.env.GB_PROXY) {
  app.commandLine.appendSwitch('proxy-server', process.env.GB_PROXY);
}

let win = null;

// 单实例：重复启动时聚焦已有窗口
if (!app.requestSingleInstanceLock()) app.quit();

// ---------- 注入到页面的 JS ----------

// 聚焦 Gemini 输入框（contenteditable 或 textarea，通用探测）
const focusInputJs = `
(() => {
  const cands = [...document.querySelectorAll('textarea, [contenteditable="true"]')];
  const el = cands.find((e) => e.getClientRects().length > 0);
  if (!el) return false;
  el.focus();
  try {
    const sel = getSelection();
    const range = document.createRange();
    range.selectNodeContents(el);
    range.collapse(false);
    sel.removeAllRanges();
    sel.addRange(range);
  } catch (_) {}
  return true;
})()
`;

// 判断当前焦点是否在"有内容"的输入框里（决定 Esc 是隐藏窗口还是放行给页面）
const escapeCheckJs = `
(() => {
  const a = document.activeElement;
  if (!a) return false;
  const editable = a.isContentEditable || a.tagName === 'TEXTAREA' || a.tagName === 'INPUT';
  if (!editable) return false;
  return ((a.innerText ?? a.value ?? '') + '').trim().length > 0;
})()
`;

// 紧凑化微调：顶部左侧留一条可拖动区域（右上角按钮不受影响）
const cssTweaks = `
body::after {
  content: '';
  position: fixed;
  top: 0; left: 0;
  width: 65%; height: 9px;
  z-index: 2147483647;
  -webkit-app-region: drag;
}
`;

// ---------- 窗口 ----------

function createWindow() {
  win = new BrowserWindow({
    width: WIN_WIDTH,
    height: WIN_HEIGHT,
    show: false,
    frame: false,
    fullscreenable: false,
    alwaysOnTop: true,
    backgroundColor: '#1b1c1d',
    webPreferences: {
      partition: PARTITION,
      contextIsolation: true,
      nodeIntegration: false,
      spellcheck: false,
    },
  });

  win.loadURL(GEMINI_URL);

  win.webContents.on('did-finish-load', () => {
    win.webContents.insertCSS(cssTweaks).catch(() => {});
  });

  // Google 登录弹窗：允许弹出并复用同一 session；其他外链交给系统浏览器
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https:\/\/accounts\.google\.com/.test(url)) {
      return {
        action: 'allow',
        overrideBrowserWindowOptions: {
          width: 500,
          height: 660,
          fullscreenable: false,
          webPreferences: { partition: PARTITION },
        },
      };
    }
    shell.openExternal(url);
    return { action: 'deny' };
  });

  // Esc：输入框为空时隐藏窗口；有内容时放行给页面（如停止生成）
  win.webContents.on('before-input-event', (_e, input) => {
    if (input.type === 'keyDown' && input.key === 'Escape') {
      win.webContents
        .executeJavaScript(escapeCheckJs, true)
        .then((hasText) => { if (!hasText) win.hide(); })
        .catch(() => win.hide());
    }
  });

  win.on('closed', () => { win = null; });
}

// 窗口出现在鼠标所在屏幕，居中偏上
function positionWindow() {
  if (!win) return;
  const disp = screen.getDisplayNearestPoint(screen.getCursorScreenPoint());
  const wa = disp.workArea;
  win.setPosition(
    wa.x + Math.round((wa.width - WIN_WIDTH) / 2),
    wa.y + Math.round(wa.height * 0.12)
  );
}

function toggleWindow() {
  if (!win) createWindow();
  if (win.isVisible()) {
    win.hide();
    return;
  }
  positionWindow();
  win.show();
  win.focus();
  setTimeout(() => {
    win?.webContents.executeJavaScript(focusInputJs, true).catch(() => {});
  }, 150);
}

// ---------- 生命周期 ----------

app.whenReady().then(() => {
  if (process.platform === 'darwin') app.dock.hide(); // 不占 Dock，纯快捷键工具

  createWindow(); // 预加载页面，呼出即达

  const ok = globalShortcut.register(TOGGLE_ACCELERATOR, toggleWindow);
  if (!ok) {
    new Notification({
      title: 'Gemini Buddy',
      body: `快捷键 ${TOGGLE_ACCELERATOR} 注册失败，可能被其他应用占用，请修改 main.js 中的 TOGGLE_ACCELERATOR`,
    }).show();
    console.error(`[gemini-buddy] 快捷键 ${TOGGLE_ACCELERATOR} 注册失败`);
  }

  // 调试截图：GB_DEBUG_SHOT=<png路径> 启动，自动显示窗口截图后退出
  if (process.env.GB_DEBUG_SHOT) {
    const wait = Number(process.env.GB_DEBUG_WAIT || 9000);
    setTimeout(async () => {
      try {
        positionWindow();
        win.show();
        await new Promise((r) => setTimeout(r, 1500));
        const img = await win.webContents.capturePage();
        fs.writeFileSync(process.env.GB_DEBUG_SHOT, img.toPNG());
        console.log('[gemini-buddy] 截图已保存:', process.env.GB_DEBUG_SHOT);
      } catch (err) {
        console.error('[gemini-buddy] 截图失败:', err);
      }
      app.quit();
    }, wait);
  }
});

app.on('second-instance', () => toggleWindow());
app.on('will-quit', () => globalShortcut.unregisterAll());
app.on('window-all-closed', () => {}); // 常驻后台，不随窗口关闭退出
