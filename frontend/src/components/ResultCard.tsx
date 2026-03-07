import type { Facility } from '../types'

interface Props {
  facility: Facility
}

export default function ResultCard({ facility }: Props) {
  return (
    <div style={{ border: '1px solid #ccc', borderRadius: 4, padding: '0.75rem 1rem' }}>
      <strong>{facility.name}</strong>
      {facility.description && <p style={{ margin: '0.25rem 0 0' }}>{facility.description}</p>}
    </div>
  )
}
