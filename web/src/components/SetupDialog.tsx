import { useEffect, useId, useState } from 'react'

interface Props { title: string; initialValue: string; onClose: () => void; onSubmit: (value: string) => void; submitLabel?: string; readOnly?: boolean }

export function SetupDialog({ title, initialValue, onClose, onSubmit, submitLabel = 'Load', readOnly = false }: Props) {
  const [value, setValue] = useState(initialValue)
  const titleId = useId()
  const fieldId = useId()
  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => { if (event.key === 'Escape') onClose() }
    window.addEventListener('keydown', closeOnEscape)
    return () => window.removeEventListener('keydown', closeOnEscape)
  }, [onClose])
  return <div className="modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose() }}><form className="modal" role="dialog" aria-modal="true" aria-labelledby={titleId} onSubmit={(event) => { event.preventDefault(); onSubmit(value) }}><div className="modal__heading"><h2 id={titleId}>{title}</h2><button type="button" onClick={onClose} aria-label={`Close ${title}`}>×</button></div><label className="sr-only" htmlFor={fieldId}>{title}</label><textarea id={fieldId} value={value} onChange={(event) => setValue(event.target.value)} readOnly={readOnly} autoFocus /><div className="modal__actions">{!readOnly && <button type="button" onClick={onClose}>Cancel</button>}<button className="primary" type="submit">{submitLabel}</button></div></form></div>
}
