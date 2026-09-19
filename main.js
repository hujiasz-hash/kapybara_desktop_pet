/**
 * Gemini Buddy v0.3 — 跨平台（macOS/Windows）快捷键呼出的 Gemini 快速问答浮窗
 *
 * - Option+G (mac) / Alt+G (Win) 呼出/隐藏，Electron 系统级快捷键，无需辅助功能授权
 * - 透明无边框对话框，打字提问，agy（Antigravity CLI）流式回答，每次覆盖上一条
 * - 失焦自动隐藏；回答生成中不隐藏，答完停 4 秒让用户看到
 * - 后端: agy -p "问题"，复用 Google 账号登录态，免 API key
 */
const { app, BrowserWindow, globalShortcut, ipcMain, screen, Notification } = require('electron');
const { spawn, execSync } = require('child_process');
const path = require('path');
const fs = require('fs');
const os = require('os');

const isMac = process.platform === 'darwin';
const HOTKEY = isMac ? 'Option+G' : 'Alt+G';
const WIN_W = 640;
const WIN_H = 400;
const ANSWER_TIMEOUT_MS = 180000;
// agy 思考档位：low 最快（可选 gemini-3.8-flash-low/medium/high，置空则用 agy 默认）
const AGY_MODEL = process.env.GB_AGY_MODEL || 'gemini-3.8-flash-low';

let win = null;
let busy = false;
let childProc = null;

// 可选代理（须在 app ready 前）
if (process.env.GB_PROXY) {
  app.commandLine.appendSwitch('proxy-server', process.env.GB_PROXY);
}

if (!app.requestSingleInstanceLock()) app.quit();

// ---------- 后端选择 ----------
// 默认 pollinations（零注册零 key，1~2 秒出答案）；要质量用 GB_BACKEND=agy（需登录，9~11 秒）
// GB_BACKEND=agy|pollinations 可强制指定
const BACKEND = process.env.GB_BACKEND || 'pollinations';

// ---------- pollinations：免费无 key，GET 即返回文本 ----------
async function runPollinations(question) {
  busy = true;
  const send = (t) => { if (win && !win.isDestroyed()) win.webContents.send('answer-chunk', t); };
  const done = () => {
    busy = false;
    if (win && !win.isDestroyed()) win.webContents.send('answer-done');
    setTimeout(() => {
      if (win && win.isVisible() && !win.isFocused()) win.hide();
    }, 4000);
  };
  try {
    const url = 'https://text.pollinations.ai/' + encodeURIComponent(question);
    const resp = await fetch(url, { signal: AbortSignal.timeout(60000) });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    const text = (await resp.text()).trim();
    console.log(`[gemini-buddy][pollinations] 返回长度=${text.length}, 发送 chunk`);
    send(text || '[空回答]');
  } catch (e) {
    send(`[出错] ${e.message}（后端: pollinations）\n`);
  }
  done();
}

// ---------- agy 定位 ----------
function findAgy() {
  try {
    const out = execSync(isMac ? 'which agy' : 'where agy', { encoding: 'utf8' });
    const first = out.trim().split(/\r?\n/)[0];
    if (first) return first;
  } catch (_) { /* PATH 里没有，继续找常见位置 */ }
  const candidates = isMac ? [
    '/opt/homebrew/bin/agy',
    '/usr/local/bin/agy',
    path.join(os.homedir(), '.local/bin/agy'),
  ] : [
    path.join(os.homedir(), 'AppData', 'Local', 'Programs', 'antigravity-cli', 'agy.exe'),
    path.join(os.homedir(), '.local', 'bin', 'agy.exe'),
  ];
  for (const c of candidates) if (fs.existsSync(c)) return c;
  return null;
}

// ---------- 窗口 ----------
function createWindow() {
  win = new BrowserWindow({
    width: WIN_W,
    height: WIN_H,
    show: false,
    frame: false,
    transparent: true,
    hasShadow: false,
    fullscreenable: false,
    resizable: true,
    alwaysOnTop: true,
    skipTaskbar: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });
  if (isMac) {
    win.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });
    app.dock.hide();
  }
  win.loadFile(path.join(__dirname, 'ui', 'index.html'));

  // 失焦自动隐藏（回答生成中除外）
  win.on('blur', () => {
    if (!busy && win.isVisible()) win.hide();
  });
  win.on('closed', () => { win = null; });

  // 卡皮巴拉眼睛跟随：推送全局鼠标 + 窗口位置
  setInterval(() => {
    if (win && !win.isDestroyed() && win.isVisible()) {
      const p = screen.getCursorScreenPoint();
      const [wx, wy] = win.getPosition();
      win.webContents.send('cursor', { mx: p.x, my: p.y, wx, wy });
    }
  }, 120);
}

function positionAtCursor() {
  const disp = screen.getDisplayNearestPoint(screen.getCursorScreenPoint());
  const wa = disp.workArea;
  win.setPosition(
    wa.x + Math.round((wa.width - WIN_W) / 2),
    wa.y + Math.round(wa.height * 0.14)
  );
}

function toggle() {
  if (!win) createWindow();
  if (win.isVisible()) {
    win.hide();
    return;
  }
  positionAtCursor();
  win.show();
  win.focus();
  win.webContents.send('focus-input');
}

// ---------- agy 调用（流式） ----------
function runAgy(question) {
  const AGY = findAgy();
  busy = true;
  const send = (channel, text) => {
    if (win && !win.isDestroyed()) win.webContents.send(channel, text);
  };

  if (!AGY) {
    busy = false;
    send('answer-chunk', '[未找到 agy 命令] 请先安装 Antigravity CLI 并登录（见 README）\n');
    send('answer-done');
    return;
  }

  let out = '';
  const args = [
    ...(AGY_MODEL ? ['--model', AGY_MODEL] : []),
    '-p', question,
  ];
  childProc = spawn(AGY, args, { timeout: ANSWER_TIMEOUT_MS });
  childProc.stdout.setEncoding('utf8');
  childProc.stdout.on('data', (d) => {
    out += d;
    send('answer-chunk', d);
  });
  childProc.stderr.on('data', (d) => {
    if (!out.trim()) send('answer-chunk', d.toString());
  });
  childProc.on('error', (e) => send('answer-chunk', `\n[调用失败] ${e.message}\n`));
  childProc.on('close', (code) => {
    busy = false;
    childProc = null;
    if (code !== 0 && !out.trim()) {
      send('answer-chunk', `\n[出错] agy 退出码 ${code}，可运行 "agy" 检查登录状态\n`);
    }
    send('answer-done');
    // 答完时用户已切走：停留几秒后自动隐藏
    setTimeout(() => {
      if (win && win.isVisible() && !win.isFocused()) win.hide();
    }, 4000);
  });
}

// ---------- IPC ----------
ipcMain.handle('ask', (_e, q) => {
  const question = String(q || '').trim();
  if (!question || busy) return { ok: false, reason: busy ? 'busy' : 'empty' };
  if (BACKEND === 'pollinations') runPollinations(question);
  else runAgy(question);
  return { ok: true };
});
ipcMain.on('hide', () => { if (win) win.hide(); });
ipcMain.handle('stop', () => {
  if (childProc) childProc.kill();
});

// ---------- 生命周期 ----------
app.whenReady().then(() => {
  createWindow(); // 预加载，呼出即达
  console.log(`[gemini-buddy] 后端: ${BACKEND}` + (BACKEND === 'agy' ? ` (${findAgy() || '未找到!'})` : ' (免费网关，约1~2秒)'));

  const ok = globalShortcut.register(HOTKEY, toggle);
  if (!ok) {
    new Notification({
      title: 'Gemini Buddy',
      body: `快捷键 ${HOTKEY} 注册失败，可能被占用，请改 main.js 里的 HOTKEY`,
    }).show();
  }

  // 自动化测试：GB_TEST_ASK="问题" 启动，自动呼出、提问、打印回答后退出
  // （与 GB_DEBUG_SHOT 互斥：截图模式自带提问逻辑）
  if (process.env.GB_TEST_ASK !== undefined && !process.env.GB_DEBUG_SHOT) {
    const q = process.env.GB_TEST_ASK || '1+1等于几？一句话回答';
    setTimeout(async () => {
      toggle();
      await new Promise((r) => setTimeout(r, 800));
      await win.webContents.executeJavaScript(
        `document.getElementById('q').value = ${JSON.stringify(q)}; submit(); true;`
      );
      // 若 GB_TEST_ASK2 存在：第一答完成后再问第二个问题，验证覆盖
      const q2 = process.env.GB_TEST_ASK2;
      await new Promise((r) => setTimeout(r, 500));
      if (q2) {
        for (let i = 0; i < 60; i++) {
          const d = await win.webContents.executeJavaScript('window.__test.done');
          if (d) break;
          await new Promise((r) => setTimeout(r, 1000));
        }
        await win.webContents.executeJavaScript(
          `document.getElementById('q').value = ${JSON.stringify(q2)}; submit(); true;`
        );
      }
      // 轮询直到回答完成
      for (let i = 0; i < 120; i++) {
        await new Promise((r) => setTimeout(r, 1000));
        const done = await win.webContents.executeJavaScript(
          `window.__test.done && document.getElementById('a').innerText`
        );
        if (done) {
          console.log('---- 回答 ----\n' + String(done).slice(-1200));
          break;
        }
      }
      app.quit();
    }, 2500);
  }

  // 调试截图：GB_DEBUG_SHOT=<png路径>（可选配 GB_TEST_ASK 先提交一个问题，抓思考动画）
  if (process.env.GB_DEBUG_SHOT) {
    setTimeout(async () => {
      toggle();
      await new Promise((r) => setTimeout(r, 2000));
      if (process.env.GB_TEST_ASK !== undefined) {
        await win.webContents.executeJavaScript(
          `document.getElementById('q').value = ${JSON.stringify(process.env.GB_TEST_ASK)}; submit(); true;`
        );
        await new Promise((r) => setTimeout(r, 900));
      }
      const img = await win.webContents.capturePage();
      fs.writeFileSync(process.env.GB_DEBUG_SHOT, img.toPNG());
      console.log('截图已保存:', process.env.GB_DEBUG_SHOT);
      app.quit();
    }, 5000);
  }
});

app.on('second-instance', () => toggle());
app.on('will-quit', () => globalShortcut.unregisterAll());
app.on('window-all-closed', () => {}); // 常驻后台
