import type { Facility } from '../../types'
import './ResultCard.css'

interface Props {
  facility: Facility
}

export default function ResultCard({ facility }: Props) {
  return (
    <div className="result-card">
      <div className="card-header">
        <div className="title-wrapper">
          <svg className="facility-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12 2L2 22h20L12 2z"></path>
            <path d="M12 22V12"></path>
          </svg>
          <h3 className="card-title">{facility.name}</h3>
        </div>
        {facility.id && <span className="card-id">ID: {facility.id}</span>}
      </div>
      
      {facility.description && (
        <div className="card-desc">
          <p>{facility.description}</p>
        </div>
      )}
    </div>
  )
}