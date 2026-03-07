export interface GeocodeResult {
  lat: number
  lon: number
  displayName: string
}

// Simple in-memory cache to prevent duplicate API calls
// Keys will be formatted as "city,state"
const geocodeCache = new Map<string, GeocodeResult>()

export async function geocodeCity(city: string, state?: string): Promise<GeocodeResult | null> {
  // Normalize the cache key (e.g., "denver,co" or "denver,")
  const cacheKey = `${city.trim().toLowerCase()},${state?.trim().toLowerCase() || ''}`
  
  if (geocodeCache.has(cacheKey)) {
    console.log(`Geocode cache hit for: ${cacheKey}`)
    return geocodeCache.get(cacheKey)!
  }

  // Use URLSearchParams for clean, structured query building
  const params = new URLSearchParams({
    city: city.trim(),
    country: 'USA',
    format: 'json',
    limit: '1'
  })
  
  if (state) {
    params.append('state', state.trim())
  }

  const url = `https://nominatim.openstreetmap.org/search?${params.toString()}`

  try {
    const res = await fetch(url, {
      headers: {
        // Essential for Nominatim's Terms of Service
        'User-Agent': 'Outrider-App/1.0', 
        'Accept-Language': 'en-US,en;q=0.9',
      },
    })
    
    if (!res.ok) throw new Error(`Geocoding failed with status: ${res.status}`)
    
    const data = await res.json()
    
    if (data && data.length > 0) {
      const result: GeocodeResult = {
        lat: parseFloat(data[0].lat),
        lon: parseFloat(data[0].lon),
        displayName: data[0].display_name,
      }
      
      // Save to cache for future lookups in this session
      geocodeCache.set(cacheKey, result)
      return result
    }
    
    return null
  } catch (err) {
    console.error("Geocoding Error:", err)
    return null
  }
}