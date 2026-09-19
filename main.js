/**
 * Capybara Buddy v1.1 — 桌面常驻像素卡皮巴拉 + 快捷键问答
 *
 * - 桌面常驻卡皮巴拉（不占焦点、置顶、可拖动），点击它弹出问答窗
 * - Option+G (mac) / Alt+G (Win) 同样呼出/隐藏问答窗
 * - 卡皮巴拉朝向跟随鼠标（左看/右看/抬头看橘子/呆立），随机踱步、打盹
 * - 后端自动选择：pollinations（零 key 免费网关，快）/ agy（GB_BACKEND=agy，需登录）
 */
const { app, BrowserWindow, globalShortcut, ipcMain, screen, Notification } = require('electron');
const { spawn, execSync } = require('child_process');
const path = require('path');
const fs = require('fs');
const os = require('os');

const isMac = process.platform === 'darwin';
const HOTKEY = isMac ? 'Option+G' : 'Alt+G';
const CHAT_W = 640;
const CHAT_H = 400;
const PET_SIZE = 110;
const ANSWER_TIMEOUT_MS = 180000;
// agy 思考档位：low 最快；置空则用 agy 默认
const AGY_MODEL = process.env.GB_AGY_MODEL || 'gemini-3.8-flash-low';

let chatWin = null;    // 问答浮窗
let petWin = null;     // 桌面宠物（常驻）
let busy = false;
let childProc = null;

// 可选代理（须在 app ready 前）
if (process.env.GB_PROXY) {
  app.commandLine.appendSwitch('proxy-server', process.env.GB_PROXY);
}

if (!app.requestSingleInstanceLock()) app.quit();

// ---------- 后端选择 ----------
// 默认 pollinations（零注册零 key，1~2 秒）；GB_BACKEND=agy 用 Antigravity CLI（质量更好）
const BACKEND = process.env.GB_BACKEND || 'pollinations';

// ---------- pollinations：免费无 key，GET 即返回文本 ----------
async function runPollinations(question) {
  busy = true;
  const send = (t) => { if (chatWin && !chatWin.isDestroyed()) chatWin.webContents.send('answer-chunk', t); };
  const done = () => {
    busy = false;
    if (chatWin && !chatWin.isDestroyed()) chatWin.webContents.send('answer-done');
    petEvent('cheering');   // 答完了欢呼庆祝
    setTimeout(() => {
      if (chatWin && chatWin.isVisible() && !chatWin.isFocused()) chatWin.hide();
    }, 4000);
  };
  try {
    const url = 'https://text.pollinations.ai/' + encodeURIComponent(question);
    const resp = await fetch(url, { signal: AbortSignal.timeout(60000) });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    const text = (await resp.text()).trim();
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

// ---------- agy 调用（流式） ----------
function runAgy(question) {
  const AGY = findAgy();
  busy = true;
  const send = (t) => {
    if (chatWin && !chatWin.isDestroyed()) chatWin.webContents.send('answer-chunk', t);
  };

  if (!AGY) {
    busy = false;
    send('[未找到 agy 命令] 请先安装 Antigravity CLI 并登录（见 README）\n');
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
    send(d);
  });
  childProc.stderr.on('data', (d) => {
    if (!out.trim()) send(d.toString());
  });
  childProc.on('error', (e) => send(`\n[调用失败] ${e.message}\n`));
  childProc.on('close', (code) => {
    busy = false;
    childProc = null;
    if (code !== 0 && !out.trim()) {
      send(`\n[出错] agy 退出码 ${code}，可运行 "agy" 检查登录状态\n`);
    }
    send('answer-done');
    // 答完时用户已切走：停留几秒后自动隐藏
    setTimeout(() => {
      if (chatWin && chatWin.isVisible() && !chatWin.isFocused()) chatWin.hide();
    }, 4000);
  });
}

// ---------- 问答浮窗 ----------
function createChatWindow() {
  chatWin = new BrowserWindow({
    width: CHAT_W,
    height: CHAT_H,
    show: false,
    frame: false,
    transparent: true,
    hasShadow: false,
    fullscreenable: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });
  if (isMac) chatWin.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });
  chatWin.loadFile(path.join(__dirname, 'ui', 'index.html'));

  // 失焦自动隐藏（回答生成中除外）
  chatWin.on('blur', () => {
    if (!busy && chatWin.isVisible()) chatWin.hide();
  });
  chatWin.on('closed', () => { chatWin = null; });
}

// 呼出：问答窗贴着宠物弹出
function toggleChat() {
  if (!chatWin) createChatWindow();
  if (chatWin.isVisible()) {
    chatWin.hide();
    return;
  }
  let px = 100, py = 100;
  if (petWin) [px, py] = petWin.getPosition();
  const disp = screen.getDisplayNearestPoint({ x: px, y: py });
  const wa = disp.workArea;
  let x = px - CHAT_W - 6;                 // 默认宠物左侧
  if (x < wa.x + 4) x = px + PET_SIZE + 6; // 左边放不下放右侧
  const y = Math.max(wa.y + 4, py - 60);
  chatWin.setPosition(x, y);
  chatWin.show();
  chatWin.focus();
  chatWin.webContents.send('focus-input');
  petEvent('offering_heart');   // 打开问答，捧爱心迎接
}

// ---------- 桌面宠物（常驻，不抢焦点） ----------
function createPetWindow() {
  petWin = new BrowserWindow({
    width: PET_SIZE,
    height: PET_SIZE,
    show: false,
    frame: false,
    transparent: true,
    hasShadow: false,
    resizable: false,
    maximizable: false,
    minimizable: false,
    fullscreenable: false,
    skipTaskbar: true,
    focusable: false,            // 点击不抢当前应用焦点
    alwaysOnTop: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });
  petWin.setAlwaysOnTop(true, 'floating');
  if (isMac) petWin.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });
  petWin.loadFile(path.join(__dirname, 'pet.html'));

  // 初始位置：主屏右上角
  const wa = screen.getPrimaryDisplay().workArea;
  petWin.setPosition(wa.x + wa.width - 130, wa.y + 70);
  petWin.showInactive();         // 显示但不夺焦点
  petWin.on('closed', () => { petWin = null; });
}

// ---------- IPC ----------
ipcMain.handle('ask', (_e, q) => {
  const question = String(q || '').trim();
  if (!question || busy) return { ok: false, reason: busy ? 'busy' : 'empty' };
  if (BACKEND === 'pollinations') runPollinations(question);
  else runAgy(question);
  return { ok: true };
});
ipcMain.on('hide', () => { if (chatWin) chatWin.hide(); });
ipcMain.handle('stop', () => {
  if (childProc) childProc.kill();
});
// 拖动：绝对坐标（按下时窗口位置 + 鼠标净位移），边缘 clamp 不会累积漂移
let petDrag = null;
ipcMain.on('pet-drag-start', (_e, sx, sy) => {
  if (!petWin) return;
  const [x, y] = petWin.getPosition();
  petDrag = { sx, sy, x, y };
});
ipcMain.on('pet-drag-move', (_e, sx, sy) => {
  if (!petDrag || !petWin) return;
  petWin.setPosition(
    Math.round(petDrag.x + sx - petDrag.sx),
    Math.round(petDrag.y + sy - petDrag.sy)
  );
});
ipcMain.on('pet-drag-end', () => { petDrag = null; });
ipcMain.on('pet-click', () => toggleChat());

function petEvent(ev) {
  if (petWin && !petWin.isDestroyed()) petWin.webContents.send('pet-event', ev);
}

// ---------- 生命周期 ----------
app.whenReady().then(() => {
  if (isMac) app.dock.hide();

  createPetWindow();   // 常驻宠物
  createChatWindow();  // 预加载问答窗，呼出即达
  console.log(`[gemini-buddy] 后端: ${BACKEND}` + (BACKEND === 'agy' ? ` (${findAgy() || '未找到!'})` : ' (免费网关，约1~2秒)'));

  const ok = globalShortcut.register(HOTKEY, toggleChat);
  if (!ok) {
    new Notification({
      title: 'Capybara Buddy',
      body: `快捷键 ${HOTKEY} 注册失败，可能被占用，请改 main.js 里的 HOTKEY`,
    }).show();
  }

  // 鼠标推送：宠物朝向跟随（120ms）
  setInterval(() => {
    const p = screen.getCursorScreenPoint();
    if (petWin && !petWin.isDestroyed()) {
      const [px, py] = petWin.getPosition();
      petWin.webContents.send('cursor', { mx: p.x, my: p.y, px, py });
    }
  }, 120);

  // 自动化测试：GB_TEST_ASK="问题"（GB_TEST_ASK2 第二问验证覆盖）
  if (process.env.GB_TEST_ASK !== undefined && !process.env.GB_DEBUG_SHOT) {
    const q = process.env.GB_TEST_ASK;
    setTimeout(async () => {
      toggleChat();
      await new Promise((r) => setTimeout(r, 800));
      await chatWin.webContents.executeJavaScript(
        `document.getElementById('q').value = ${JSON.stringify(q)}; submit(); true;`
      );
      const q2 = process.env.GB_TEST_ASK2;
      await new Promise((r) => setTimeout(r, 500));
      if (q2) {
        for (let i = 0; i < 60; i++) {
          const d = await chatWin.webContents.executeJavaScript('window.__test.done');
          if (d) break;
          await new Promise((r) => setTimeout(r, 1000));
        }
        await chatWin.webContents.executeJavaScript(
          `document.getElementById('q').value = ${JSON.stringify(q2)}; submit(); true;`
        );
      }
      for (let i = 0; i < 120; i++) {
        await new Promise((r) => setTimeout(r, 1000));
        const done = await chatWin.webContents.executeJavaScript(
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

  // 调试截图：GB_DEBUG_SHOT=<png路径>（GB_TEST_ASK 可选配：先提交一个问题）
  if (process.env.GB_DEBUG_SHOT) {
    setTimeout(async () => {
      toggleChat();
      await new Promise((r) => setTimeout(r, 2000));
      if (process.env.GB_TEST_ASK !== undefined) {
        await chatWin.webContents.executeJavaScript(
          `document.getElementById('q').value = ${JSON.stringify(process.env.GB_TEST_ASK)}; submit(); true;`
        );
        await new Promise((r) => setTimeout(r, 900));
      }
      const img = await chatWin.webContents.capturePage();
      fs.writeFileSync(process.env.GB_DEBUG_SHOT, img.toPNG());
      console.log('截图已保存:', process.env.GB_DEBUG_SHOT);
      app.quit();
    }, 5000);
  }
});

app.on('second-instance', () => toggleChat());
app.on('will-quit', () => globalShortcut.unregisterAll());
app.on('window-all-closed', () => {}); // 常驻后台
