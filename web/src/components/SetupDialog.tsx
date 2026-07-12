import { useEffect, useId, useRef, useState } from 'react'

interface Props { title: string; initialValue: string; onClose: () => void; onSubmit: (value: string) => void; submitLabel?: string; readOnly?: boolean }

export function SetupDialog({ title, initialValue, onClose, onSubmit, submitLabel = 'Load', readOnly = false }: Props) {
  const [value, setValue] = useState(initialValue)
  const titleId = useId()
  const fieldId = useId()
  const dialogRef = useRef<HTMLDialogElement>(null)
  useEffect(() => {
    dialogRef.current?.showModal()
  }, [])
  const close = () => dialogRef.current?.close()
  return <dialog ref={dialogRef} className="modal" aria-labelledby={titleId} onClose={onClose} onClick={(event) => { if (event.target === dialogRef.current) close() }}><form onSubmit={(event) => { event.preventDefault(); onSubmit(value) }}><div className="modal__heading"><h2 id={titleId}>{title}</h2><button type="button" onClick={close} aria-label={`Close ${title}`}>×</button></div><label className="sr-only" htmlFor={fieldId}>{title}</label><textarea id={fieldId} value={value} onChange={(event) => setValue(event.target.value)} readOnly={readOnly} autoFocus /><div className="modal__actions">{!readOnly && <button type="button" onClick={close}>Cancel</button>}<button className="primary" type="submit">{submitLabel}</button></div></form></dialog>
}
