import { useState } from 'react'
import MapPicker from '../MapPicker/MapPicker'
import { geocodeCity } from '../../lib/geocode'
import './Filters.css'

export interface FilterValues {
  state: string
  lat: number | null
  lon: number | null
  radius: number
  facilityType: string
}

const US_STATES = [
  'AL', 'AK', 'AZ', 'AR', 'CA', 'CO', 'CT', 'DE', 'FL', 'GA', 'HI', 'ID', 'IL', 'IN', 'IA', 'KS', 'KY', 'LA', 'ME', 'MD', 'MA', 'MI', 'MN', 'MS', 'MO', 'MT', 'NE', 'NV', 'NH', 'NJ', 'NM', 'NY', 'NC', 'ND', 'OH', 'OK', 'OR', 'PA', 'RI', 'SC', 'SD', 'TN', 'TX', 'UT', 'VT', 'VA', 'WA', 'WV', 'WI', 'WY'
]

interface Props {
  onApply: (filters: FilterValues) => void
  loading: boolean
}

export default function Filters({ onApply, loading }: Props) {
  const [state, setState] = useState('')
  const [city, setCity] = useState('')
  const [radius, setRadius] = useState(50)
  const [facilityType, setFacilityType] = useState('')
  const [mapOpen, setMapOpen] = useState(false)
  const [pinPosition, setPinPosition] = useState<[number, number] | null>(null)
  
  const handleApply = async () => {
    let lat: number | null = null
    let lon: number | null = null

    // If a pin is selected, use it. Otherwise try to geocode the city.
    if (pinPosition) {
      lat = pinPosition[0]
      lon = pinPosition[1]
    } else if (city.trim() !== '') {
      const geo = await geocodeCity(city, state)
      if (geo) {
        lat = geo.lat
        lon = geo.lon
      } else {
        alert("Could not geocode the city. Please check the spelling.")
        return
      }
    }

    onApply({
      state,
      lat,
      lon,
      radius: lat !== null ? radius : 50,
      facilityType: facilityType.trim()
    })
  }

  return (
    <div className="filters-glass-panel">
      <h3>Search Filters</h3>
      <div className="filters-grid">
        <div className="filter-group">
          <label>State</label>
          <select value={state} onChange={e => setState(e.target.value)} disabled={loading}>
            <option value="">Any State</option>
            {US_STATES.map(s => <option key={s} value={s}>{s}</option>)}
          </select>
        </div>
        <div className="filter-group">
          <label>Facility Type</label>
          <input 
            type="text" 
            placeholder="e.g. Campground, Picnic Area" 
            value={facilityType} 
            onChange={e => setFacilityType(e.target.value)}
            disabled={loading}
          />
        </div>
      </div>

      <div className="location-section">
        <h4>Location (Radius Search)</h4>
        <div className="filters-grid">
          <div className="filter-group">
            <label>City</label>
            <input 
              type="text" 
              placeholder="e.g. Denver" 
              value={city} 
              onChange={e => {
                setCity(e.target.value); 
                if (e.target.value !== '') setPinPosition(null); // Clear map pin if typing city
              }}
              disabled={loading || pinPosition !== null}
            />
          </div>
          <div className="filter-group">
            <label>Radius ({radius} miles)</label>
            <input 
              type="range" 
              min={5} max={500} step={5} 
              value={radius} 
              onChange={e => setRadius(Number(e.target.value))} 
              disabled={loading || (city === '' && pinPosition === null)}
            />
          </div>
        </div>
        <div className="map-toggle-wrapper">
          <button 
            className="btn-secondary" 
            onClick={() => { setMapOpen(!mapOpen); if(!mapOpen) setCity('') }}
            disabled={loading}
          >
            {mapOpen ? 'Close Map' : 'Select on Map Instead'}
          </button>
          {pinPosition && <span className="pin-text">Pin Selected: {pinPosition[0].toFixed(2)}, {pinPosition[1].toFixed(2)}</span>}
        </div>
        
        {mapOpen && (
          <MapPicker position={pinPosition} onChange={setPinPosition} />
        )}
      </div>

      <button className="btn-primary apply-btn" onClick={handleApply} disabled={loading}>
        Apply Filters
      </button>
    </div>
  )
}
