import type { Facility } from '../../types'
import ResultCard from '../ResultCard/ResultCard'
import './SearchResults.css'

interface Props {
  results: Facility[]
}

export default function SearchResults({ results }: Props) {
  if (results.length === 0) return (
    <div className="empty-results">
      <p>No facilities found. Try adjusting your filters or search query.</p>
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
