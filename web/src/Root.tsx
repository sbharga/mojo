import { useCallback, useEffect, useRef, useState } from 'react'
import App from './App'
import { ComparePage } from './comparison/ComparePage'

function currentPage() {
  return window.location.hash === '#/compare' ? 'compare' : 'play'
}

export default function Root() {
  const [page, setPage] = useState(currentPage)
  const comparisonRunning = useRef(false)
  const setComparisonRunning = useCallback((running: boolean) => {
    comparisonRunning.current = running
  }, [])

  useEffect(() => {
    const update = () => {
      const nextPage = currentPage()
      if (
        page === 'compare'
        && nextPage !== 'compare'
        && comparisonRunning.current
        && !window.confirm('Leave this page and cancel the running match?')
      ) {
        window.history.replaceState(null, '', '#/compare')
        return
      }
      setPage(nextPage)
    }
    window.addEventListener('hashchange', update)
    return () => window.removeEventListener('hashchange', update)
  }, [page])
  return page === 'compare' ? <ComparePage onRunningChange={setComparisonRunning} /> : <App />
}
