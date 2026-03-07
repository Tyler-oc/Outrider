import { MapContainer, TileLayer, Marker, useMapEvents } from 'react-leaflet'
import 'leaflet/dist/leaflet.css'
import L from 'leaflet'
import './MapPicker.css'

// We don't need the default blue PNGs anymore! 
// We will create a custom HTML marker that uses our CSS theme colors.
const customMarkerIcon = L.divIcon({
  className: 'custom-map-pin',
  html: `<div class="pin-head"></div><div class="pin-point"></div>`,
  iconSize: [24, 34],
  iconAnchor: [12, 34]
})

interface Props {
  position: [number, number] | null
  onChange: (pos: [number, number]) => void
}

function LocationMarker({ position, onChange }: Props) {
  useMapEvents({
    click(e) {
      onChange([e.latlng.lat, e.latlng.lng])
    },
  })

  return position === null ? null : (
    <Marker position={position} icon={customMarkerIcon} />
  )
}

export default function MapPicker({ position, onChange }: Props) {
  const defaultCenter: [number, number] = [39.8283, -98.5795] // US center

  return (
    <div className="map-picker-container">
      <MapContainer 
        center={position || defaultCenter} 
        zoom={position ? 8 : 4} 
        scrollWheelZoom={true} 
        style={{ height: '100%', width: '100%', borderRadius: 'inherit' }}
      >
        {/* Swapped to CartoDB Dark Matter tiles for a sleek dark mode look */}
        <TileLayer
          attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors &copy; <a href="https://carto.com/attributions">CARTO</a>'
          url="https://{s}.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}{r}.png"
        />
        <LocationMarker position={position} onChange={onChange} />
      </MapContainer>
    </div>
  )
}