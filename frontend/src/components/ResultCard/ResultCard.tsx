import type { Facility } from '../../types'
import './ResultCard.css'

interface Props {
  facility: Facility
}

export default function ResultCard({ facility }: Props) {
  return (
    <div className="result-card">
      <div className="card-header">
        <h3 className="card-title">{facility.name}</h3>
        {facility.id && <span className="card-id">ID: {facility.id}</span>}
      </div>
      {facility.description && (
        <div className="card-desc">{facility.description}</div>
      )}
    </div>
  )
}
