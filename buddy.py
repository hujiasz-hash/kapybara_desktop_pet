#!/usr/bin/env python3
"""
Gemini Buddy（轻量版）
Option+G 呼出/隐藏一个置顶小窗，问一句答一句；点窗口外自动隐藏。
后端调用 Antigravity CLI（agy -p，复用 Google 账号登录态，免 API key）。

依赖：python3(自带 tkinter)、pynput(pip3 install pynput)、agy
"""

import os
import queue
import shutil
import subprocess
import sys
import threading
import time
import tkinter as tk

# ---------- 配置 ----------
HOTKEY = '<alt>+g'             # Option+G 呼出/隐藏，可改（如 '<ctrl>+<alt>+g'）
WIN_W, WIN_H = 640, 440
FONT = 'PingFang SC'
ANSWER_TIMEOUT = 180           # 单次回答超时（秒）

BG     = '#1e1f22'             # 面板底色
BORDER = '#3c4043'             # 描边
FG     = '#e8eaed'             # 主文字
DIM    = '#9aa0a6'             # 次要文字
ACCENT = '#8ab4f8'             # 问题/强调
ERR    = '#f28b82'             # 错误


def find_agy():
    """定位 agy。nohup/launchd 启动时 PATH 可能不全，逐个兜底。"""
    p = shutil.which('agy')
    if p:
        return p
    for cand in (
        os.path.expanduser('~/.local/bin/agy'),
        '/opt/homebrew/bin/agy',
        '/usr/local/bin/agy',
    ):
        if os.path.exists(cand):
            return cand
    return 'agy'


class Buddy:
    def __init__(self):
        self.agy = find_agy()
        self.busy = False
        self.visible = False
        self.q = queue.Queue()          # 跨线程 -> tk 主线程 事件队列
        self._drag_dx = self._drag_dy = 0

        r = self.root = tk.Tk()
        r.title('gemini-buddy')
        r.configure(bg=BG)
        r.overrideredirect(True)        # 无边框
        r.attributes('-topmost', True)
        r.withdraw()

        outer = tk.Frame(r, bg=BORDER)  # 1px 描边
        outer.pack(fill='both', expand=True, padx=1, pady=1)

        # ---- 顶部：标题（兼拖动区）----
        top = tk.Frame(outer, bg=BG)
        top.pack(fill='x', padx=14, pady=(9, 6))
        title = tk.Label(top, text='✦ Gemini Buddy', bg=BG, fg=DIM, font=(FONT, 11))
        title.pack(side='left')
        for w in (top, title):
            w.bind('<Button-1>', self._drag_start)
            w.bind('<B1-Motion>', self._drag_move)

        # ---- 输入框 ----
        self.entry = tk.Entry(
            outer, font=(FONT, 14), bg='#2a2c2f', fg=FG,
            insertbackground=FG, relief='flat',
            highlightthickness=1, highlightbackground=BORDER, highlightcolor=ACCENT,
        )
        self.entry.pack(fill='x', padx=14, pady=(0, 8), ipady=7)
        self.entry.bind('<Return>', self.ask)
        self.entry.bind('<Escape>', lambda e: self.hide())

        # ---- 回答区 ----
        mid = tk.Frame(outer, bg=BG)
        mid.pack(fill='both', expand=True, padx=14)
        self.text = tk.Text(
            mid, font=(FONT, 13), bg=BG, fg=FG, relief='flat',
            wrap='word', state='disabled', padx=2, pady=4,
            selectbackground='#3c4b66', spacing3=3,
        )
        sb = tk.Scrollbar(mid, command=self.text.yview, width=10)
        self.text.configure(yscrollcommand=sb.set)
        sb.pack(side='right', fill='y')
        self.text.pack(side='left', fill='both', expand=True)
        self.text.tag_configure('q', foreground=ACCENT, font=(FONT, 13, 'bold'))
        self.text.tag_configure('a', foreground=FG)
        self.text.tag_configure('err', foreground=ERR)
        self.text.tag_configure('dim', foreground=DIM)
        self._append('问点什么吧，回车发送。\n', 'dim')
        self._append('Option+G 呼出/隐藏 · Esc 关闭 · 点窗口外自动隐藏', 'dim')

        # ---- 状态栏 ----
        self.status = tk.Label(outer, text='就绪', bg=BG, fg=DIM,
                               font=(FONT, 10), anchor='e')
        self.status.pack(fill='x', padx=14, pady=(4, 8))

        # 失焦自动隐藏（正在生成回答时不隐藏，答完打在屏幕上）
        r.bind('<Deactivate>', self._on_deactivate)
        r.bind('<Escape>', lambda e: self.hide())

        # ---- 全局快捷键（后台线程 -> 队列 -> tk 主线程）----
        try:
            from pynput import keyboard
            hk = keyboard.GlobalHotKeys({HOTKEY: lambda: self.q.put(self.toggle)})
            hk.daemon = True
            hk.start()
        except Exception as e:
            print(f'[gemini-buddy] 全局快捷键不可用: {e}\n'
                  '  提示: pip3 install pynput；并在 系统设置→隐私与安全性→辅助功能 '
                  '里给运行本脚本的终端勾选授权。', file=sys.stderr)

        self.root.after(60, self._pump)

    # ---------- 窗口 ----------
    def _place(self):
        x = self.root.winfo_pointerx() - WIN_W // 2
        y = self.root.winfo_pointery() - 220            # 鼠标处居中偏上
        sw, sh = self.root.winfo_screenwidth(), self.root.winfo_screenheight()
        x = max(10, min(x, sw - WIN_W - 10))
        y = max(10, min(y, sh - WIN_H - 10))
        self.root.geometry(f'{WIN_W}x{WIN_H}+{x}+{y}')

    def show(self, focus=True):
        self._place()
        self.root.deiconify()
        self.root.lift()
        if focus:
            self.root.focus_force()
            self.entry.focus_set()
            self.entry.icursor('end')
        self.visible = True

    def hide(self):
        self.root.withdraw()
        self.visible = False

    def toggle(self):
        self.hide() if self.visible else self.show()

    def _drag_start(self, e):
        self._drag_dx, self._drag_dy = e.x, e.y

    def _drag_move(self, e):
        self.root.geometry(f'+{e.x_root - self._drag_dx}+{e.y_root - self._drag_dy}')

    def _on_deactivate(self, _e):
        if not self.busy:
            self.hide()

    # ---------- 问答 ----------
    def ask(self, _event=None):
        q = self.entry.get().strip()
        if not q or self.busy:
            return
        self.entry.delete(0, 'end')
        self._append(f'\nQ  {q}\n', 'q')
        self._set_status('思考中…')
        self.busy = True
        threading.Thread(target=self._worker, args=(q,), daemon=True).start()

    def _worker(self, q):
        t0 = time.time()
        ok = True
        try:
            r = subprocess.run(
                [self.agy, '-p', q],
                capture_output=True, text=True, timeout=ANSWER_TIMEOUT,
            )
            ans = (r.stdout or '').strip()
            if not ans:
                ok = False
                ans = (r.stderr or f'agy 退出码 {r.returncode}，无输出').strip()[:400]
        except Exception as e:
            ok = False
            ans = f'调用失败: {e}'
        dt = time.time() - t0
        self.q.put(lambda: self._on_answer(ans, dt, ok))

    def _on_answer(self, ans, dt, ok):
        self.busy = False
        self._append(f'A  {ans}\n', 'a' if ok else 'err')
        self._set_status(f'{"完成" if ok else "出错"} · {dt:.1f}s')
        # 用户已切走时把答案弹到屏幕上（不抢键盘焦点）
        if not self.visible:
            self.show(focus=False)

    # ---------- 杂项 ----------
    def _append(self, text, tag):
        t = self.text
        t.configure(state='normal')
        t.insert('end', text, tag)
        t.configure(state='disabled')
        t.see('end')

    def _set_status(self, s):
        self.status.configure(text=s)

    def _pump(self):
        """把其他线程投递的动作调度到 tk 主线程执行。"""
        try:
            while True:
                fn = self.q.get_nowait()
                fn()
        except queue.Empty:
            pass
        self.root.after(60, self._pump)


def self_test(question):
    """自动化验证：建 UI、走一遍完整问答、打印结果后退出。"""
    b = Buddy()
    b.show(focus=False)
    b.root.update()
    b.entry.insert(0, question)
    b.ask()
    while b.busy:
        b.root.update()
        time.sleep(0.1)
    b.root.update()
    content = b.text.get('1.0', 'end').strip()
    print('---- 回答区内容 ----')
    print(content[-1500:])
    b.root.destroy()
    print('---- 测试通过 ----')


if __name__ == '__main__':
    if '--test' in sys.argv:
        q = sys.argv[sys.argv.index('--test') + 1] if len(sys.argv) > sys.argv.index('--test') + 1 \
            else '1+1等于几？一句话回答'
        self_test(q)
    else:
        Buddy().root.mainloop()
