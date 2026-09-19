const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('buddy', {
  ask: (q) => ipcRenderer.invoke('ask', q),
  hide: () => ipcRenderer.send('hide'),
  stop: () => ipcRenderer.invoke('stop'),
  petDragStart: (sx, sy) => ipcRenderer.send('pet-drag-start', sx, sy),
  petDragMove: (sx, sy) => ipcRenderer.send('pet-drag-move', sx, sy),
  petDragEnd: () => ipcRenderer.send('pet-drag-end'),
  petClick: () => ipcRenderer.send('pet-click'),
  onFocusInput: (cb) => ipcRenderer.on('focus-input', () => cb()),
  onAnswerChunk: (cb) => ipcRenderer.on('answer-chunk', (_e, t) => cb(t)),
  onAnswerDone: (cb) => ipcRenderer.on('answer-done', () => cb()),
  onCursor: (cb) => ipcRenderer.on('cursor', (_e, pos) => cb(pos)),
  onPetEvent: (cb) => ipcRenderer.on('pet-event', (_e, ev) => cb(ev)),
});
