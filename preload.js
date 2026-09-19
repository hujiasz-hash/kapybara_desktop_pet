const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('buddy', {
  ask: (q) => ipcRenderer.invoke('ask', q),
  hide: () => ipcRenderer.send('hide'),
  stop: () => ipcRenderer.invoke('stop'),
  onFocusInput: (cb) => ipcRenderer.on('focus-input', () => cb()),
  onAnswerChunk: (cb) => ipcRenderer.on('answer-chunk', (_e, t) => cb(t)),
  onAnswerDone: (cb) => ipcRenderer.on('answer-done', () => cb()),
});
