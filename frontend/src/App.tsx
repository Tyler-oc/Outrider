import { useState } from 'react'
import type { Facility } from './types'
import SearchBar from './components/SearchBar/SearchBar'
import SearchResults from './components/SearchResults/SearchResults'
import Filters, { type FilterValues } from './components/Filters/Filters'
import './App.css'

function App() {
  const [results, setResults] = useState<Facility[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  
  // App-level filter state
  const [filters, setFilters] = useState<FilterValues>({
    state: '',
    lat: null,
    lon: null,
    radius: 50,
    facilityType: ''
  })

  const handleSearch = async (query: string) => {
    setLoading(true)
    setError(null)
    try {
      let url = `/search?q=${encodeURIComponent(query)}`
      if (filters.state) url += `&state=${encodeURIComponent(filters.state)}`
      if (filters.facilityType) url += `&facility_type=${encodeURIComponent(filters.facilityType)}`
      if (filters.lat !== null && filters.lon !== null) {
        url += `&lat=${filters.lat}&lon=${filters.lon}&radius_miles=${filters.radius}`
      }

      const res = await fetch(url)
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
    <div className="app-layout">
      <header className="app-header">
        <div className="header-glass">
          <h1>Outrider Facilities</h1>
          <p>Find your next outdoor adventure basecamp.</p>
        </div>
      </header>
      
      <main className="app-main">
        <aside className="sidebar">
          <Filters onApply={setFilters} loading={loading} />
        </aside>

        <section className="content">
          <div className="search-panel">
            <SearchBar onSearch={handleSearch} loading={loading} />
          </div>
          
          {error && <div className="error-badge">{error}</div>}
          
          <div className="results-panel">
            <SearchResults results={results} />
          </div>
        </section>
      </main>
    </div>
  )
}

export default App
