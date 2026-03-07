import { useState } from 'react'
import type { KeyboardEvent } from 'react'

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
    <div>
      <input
        type="text"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Search campgrounds..."
        disabled={loading}
      />
      <button onClick={submit} disabled={loading || !query.trim()}>
        {loading ? 'Searching...' : 'Search'}
      </button>
    </div>
  )
}
