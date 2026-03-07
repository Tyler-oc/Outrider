import { useState, useEffect } from 'react'
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
  const [geoError, setGeoError] = useState<string | null>(null)

  // Auto-apply effect with a 600ms debounce
  useEffect(() => {
    const timer = setTimeout(async () => {
      setGeoError(null)
      let lat: number | null = null
      let lon: number | null = null
      let appliedState = state

      // 1. If a pin is placed, use it and ignore the state string completely
      if (pinPosition) {
        lat = pinPosition[0]
        lon = pinPosition[1]
        appliedState = '' 
      } 
      // 2. Otherwise, if there is a city, try to geocode it (using state to help accuracy)
      else if (city.trim() !== '') {
        const geo = await geocodeCity(city, state)
        if (geo) {
          lat = geo.lat
          lon = geo.lon
        } else {
          setGeoError("Could not locate city. Check spelling.")
          return // Prevent firing a bad search if the city is invalid
        }
      }

      onApply({
        state: appliedState,
        lat,
        lon,
        radius: lat !== null ? radius : 50,
        facilityType: facilityType.trim()
      })
    }, 600) // Wait 600ms after the user stops interacting before firing

    return () => clearTimeout(timer)
  }, [state, city, radius, facilityType, pinPosition, onApply])

  // --- Input Handlers (Mutual Exclusivity Logic) ---

  const handleStateChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    setState(e.target.value)
    if (e.target.value !== '') setPinPosition(null) // Selecting a state clears the pin
  }

  const handleCityChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setCity(e.target.value)
    if (e.target.value !== '') setPinPosition(null) // Typing a city clears the pin
  }

  const handlePinChange = (newPin: [number, number] | null) => {
    setPinPosition(newPin)
    if (newPin) {
      setState('') // Placing a pin clears the state dropdown
      setCity('')  // Placing a pin clears the city input
    }
  }

  return (
    <div className="filters-glass-panel">
      <h3>Refine Search</h3>
      
      <div className="filters-grid">
        <div className="filter-group">
          <label>State</label>
          <select value={state} onChange={handleStateChange} disabled={loading}>
            <option value="">Any State</option>
            {US_STATES.map(s => <option key={s} value={s}>{s}</option>)}
          </select>
        </div>
        
        <div className="filter-group">
          <label>Facility Type</label>
          <input 
            type="text" 
            placeholder="e.g. Campground, Cabin" 
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
              onChange={handleCityChange}
              disabled={loading || pinPosition !== null}
            />
            {geoError && <span className="geo-error-text">{geoError}</span>}
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
            {mapOpen ? 'Close Map' : 'Drop a Pin Instead'}
          </button>
          {pinPosition && (
            <span className="pin-text">
              Pin: {pinPosition[0].toFixed(2)}, {pinPosition[1].toFixed(2)}
            </span>
          )}
        </div>
        
        {mapOpen && (
          <div className="map-container">
            <MapPicker position={pinPosition} onChange={handlePinChange} />
          </div>
        )}
      </div>
    </div>
  )
}