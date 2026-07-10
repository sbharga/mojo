import { useState } from 'react'

interface Props { title: string; initialValue: string; onClose: () => void; onSubmit: (value: string) => void }

export function SetupDialog({ title, initialValue, onClose, onSubmit }: Props) {
  const [value, setValue] = useState(initialValue)
  return <div className="modal-backdrop" role="presentation"><form className="modal" onSubmit={(event) => { event.preventDefault(); onSubmit(value) }}><div className="modal__heading"><h2>{title}</h2><button type="button" onClick={onClose}>×</button></div><textarea value={value} onChange={(event) => setValue(event.target.value)} autoFocus /><div className="modal__actions"><button type="button" onClick={onClose}>Cancel</button><button className="primary" type="submit">Load</button></div></form></div>
}
