const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('buddy', {
  ask: (q) => ipcRenderer.invoke('ask', q),
  hide: () => ipcRenderer.send('hide'),
  stop: () => ipcRenderer.invoke('stop'),
  petDragBy: (dx, dy) => ipcRenderer.send('pet-drag', dx, dy),
  petClick: () => ipcRenderer.send('pet-click'),
  onFocusInput: (cb) => ipcRenderer.on('focus-input', () => cb()),
  onAnswerChunk: (cb) => ipcRenderer.on('answer-chunk', (_e, t) => cb(t)),
  onAnswerDone: (cb) => ipcRenderer.on('answer-done', () => cb()),
  onCursor: (cb) => ipcRenderer.on('cursor', (_e, pos) => cb(pos)),
});
