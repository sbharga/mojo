// jsdom does not implement HTMLDialogElement.showModal/close (see jsdom#3294),
// so this shims just enough native <dialog> behavior for tests to exercise
// showModal/close/Escape the same way a real browser would.
if (typeof HTMLDialogElement !== 'undefined') {
  const proto = HTMLDialogElement.prototype as unknown as Record<string, unknown>
  if (typeof proto.showModal !== 'function') {
    proto.showModal = function (this: HTMLDialogElement) {
      this.setAttribute('open', '')
    }

    proto.close = function (this: HTMLDialogElement) {
      if (!this.hasAttribute('open')) return
      this.removeAttribute('open')
      this.dispatchEvent(new Event('close'))
    }

    document.addEventListener('keydown', (event) => {
      if (event.key !== 'Escape') return
      const dialog = document.querySelector('dialog[open]')
      if (!dialog) return
      const cancelEvent = new Event('cancel', { cancelable: true })
      dialog.dispatchEvent(cancelEvent)
      if (!cancelEvent.defaultPrevented) (dialog as HTMLDialogElement).close()
    })
  }
}
