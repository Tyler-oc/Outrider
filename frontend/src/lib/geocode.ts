export interface GeocodeResult {
  lat: number
  lon: number
  displayName: string
}

export async function geocodeCity(city: string, state?: string): Promise<GeocodeResult | null> {
  const query = state ? `${city}, ${state}, USA` : `${city}, USA`;
  const url = `https://nominatim.openstreetmap.org/search?q=${encodeURIComponent(
    query
  )}&format=json&limit=1`;

  try {
    const res = await fetch(url, {
      headers: {
        'User-Agent': 'Outrider-App/1.0',
        'Accept-Language': 'en-US,en;q=0.9',
      },
    });
    if (!res.ok) throw new Error('Geocoding failed');
    const data = await res.json();
    if (data && data.length > 0) {
      return {
        lat: parseFloat(data[0].lat),
        lon: parseFloat(data[0].lon),
        displayName: data[0].display_name,
      };
    }
    return null;
  } catch (err) {
    console.error(err);
    return null;
  }
}
