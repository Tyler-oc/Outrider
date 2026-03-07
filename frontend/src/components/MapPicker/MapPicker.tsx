import { MapContainer, TileLayer, Marker, useMapEvents } from 'react-leaflet'
import 'leaflet/dist/leaflet.css'
import L from 'leaflet'
import './MapPicker.css'

// Fix default icons in React-Leaflet
import icon from 'leaflet/dist/images/marker-icon.png'
import iconShadow from 'leaflet/dist/images/marker-shadow.png'

const DefaultIcon = L.icon({
  iconUrl: icon,
  shadowUrl: iconShadow,
  iconAnchor: [12, 41]
})
L.Marker.prototype.options.icon = DefaultIcon

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
    <Marker position={position}></Marker>
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
        <TileLayer
          attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors'
          url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
        />
        <LocationMarker position={position} onChange={onChange} />
      </MapContainer>
    </div>
  )
}
