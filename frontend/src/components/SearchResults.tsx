import type { Facility } from '../types'
import ResultCard from './ResultCard'

interface Props {
  results: Facility[]
}

export default function SearchResults({ results }: Props) {
  if (results.length === 0) return null

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem', marginTop: '1rem' }}>
      {results.map((facility) => (
        <ResultCard key={facility.id} facility={facility} />
      ))}
    </div>
  )
}
