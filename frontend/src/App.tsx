import { useState } from 'react'
import type { Facility } from './types'
import SearchBar from './components/SearchBar'
import SearchResults from './components/SearchResults'

function App() {
  const [results, setResults] = useState<Facility[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleSearch = async (query: string) => {
    setLoading(true)
    setError(null)
    try {
      const res = await fetch(`/search?q=${encodeURIComponent(query)}`)
      if (!res.ok) throw new Error(`Server error: ${res.status}`)
      const data: Facility[] = await res.json()
      setResults(data)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Unknown error')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div style={{ fontFamily: 'sans-serif', maxWidth: 800, margin: '2rem auto', padding: '0 1rem' }}>
      <h1>Colorado Campgrounds</h1>
      <SearchBar onSearch={handleSearch} loading={loading} />
      {error && <p style={{ color: 'red' }}>{error}</p>}
      <SearchResults results={results} />
    </div>
  )
}

export default App
