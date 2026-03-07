import type { Facility } from '../../types'
import ResultCard from '../ResultCard/ResultCard'
import './SearchResults.css'

interface Props {
  results: Facility[]
}

export default function SearchResults({ results }: Props) {
  if (results.length === 0) return (
    <div className="empty-results">
      <svg className="empty-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
        <circle cx="12" cy="12" r="10"></circle>
        <polygon points="16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76"></polygon>
      </svg>
      <p>No basecamps found in this area. Try adjusting your filters or moving your pin.</p>
    </div>
  )

  return (
    <div className="results-container">
      {results.map((facility) => (
        <ResultCard key={facility.id} facility={facility} />
      ))}
    </div>
  )
}