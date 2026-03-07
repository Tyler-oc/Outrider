import { useState } from 'react'
import type { KeyboardEvent } from 'react'
import './SearchBar.css'

interface Props {
  onSearch: (query: string) => void
  loading: boolean
}

export default function SearchBar({ onSearch, loading }: Props) {
  const [query, setQuery] = useState('')

  const submit = () => {
    if (query.trim()) onSearch(query.trim())
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') submit()
  }

  return (
    <div className="search-bar-wrapper">
      <div className="search-input-container">
        <svg className="search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <circle cx="11" cy="11" r="8"></circle>
          <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
        </svg>
        <input
          type="text"
          className="search-input"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Search for activities, views, keywords..."
          disabled={loading}
        />
      </div>
      <button className="btn-primary search-btn" onClick={submit} disabled={loading || !query.trim()}>
        {loading ? 'Searching...' : 'Search'}
      </button>
    </div>
  )
}
