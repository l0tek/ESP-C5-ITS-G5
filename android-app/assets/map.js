(async () => {
  for (let i=0; i<100 && (!window.L || !document.getElementById('traffic-map')); i++)
    await new Promise(r => setTimeout(r, 100));
  if (!window.L) { document.getElementById('map-status').textContent='Kartenbibliothek konnte nicht geladen werden'; return; }
  if (window.citsMap) return;
  const map = L.map('traffic-map', {zoomControl:false}).setView([51.16,10.45],6);
  window.citsMap = map;
  L.control.zoom({position:'topright'}).addTo(map);
  L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
    maxZoom:19, attribution:'© <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener">OpenStreetMap</a> contributors'
  }).on('tileerror', () => { document.getElementById('map-status').textContent='Kartenkacheln nicht erreichbar · Paketempfang bleibt aktiv'; })
    .on('tileload', () => { document.getElementById('map-status').textContent=''; }).addTo(map);
  const markers = new Map();
  let source = '', centered = false;
  function popup(o) {
    const element=document.createElement('div');
    const title=document.createElement('strong'); title.textContent=`${o.kind} · ${o.station_id}`; element.append(title);
    for (const text of [ `${o.latitude.toFixed(6)}, ${o.longitude.toFixed(6)}`,
      o.speed_kmh == null ? null : `${o.speed_kmh.toFixed(1)} km/h`,
      o.heading == null ? null : `Richtung ${o.heading.toFixed(1)}°`,
      `Zuletzt vor ${Math.max(0,Math.floor((Date.now()-o.received_ms)/1000))} s`, 'Signatur nicht geprüft']) {
      if (!text) continue;
      const row=document.createElement('div');row.textContent=text;element.append(row);
    }
    return element;
  }
  window.citsRender = (snapshot) => {
    if (source!==snapshot.source) { source=snapshot.source;centered=false; }
    const current=new Set();
    for (const o of Object.values(snapshot.objects)) {
      current.add(o.key);
      let marker=markers.get(o.key);
      if (!marker) {
        const warning=o.kind==='DENM';
        const icon=L.divIcon({className:'traffic-marker', html:`<span class="${warning?'event-dot':'vehicle-dot'}">${warning?'!':'↑'}</span>`,iconSize:[30,30],iconAnchor:[15,15]});
        marker=L.marker([o.latitude,o.longitude],{icon}).addTo(map);markers.set(o.key,marker);
      }
      marker.setLatLng([o.latitude,o.longitude]);marker.bindPopup(popup(o));
      const arrow=marker.getElement()?.querySelector('.vehicle-dot');
      if (arrow) arrow.style.transform=`rotate(${o.heading||0}deg)`;
    }
    for (const [key,m] of markers) if (!current.has(key)) { map.removeLayer(m);markers.delete(key); }
    if (!centered && markers.size) { map.fitBounds(L.featureGroup([...markers.values()]).getBounds().pad(.3),{maxZoom:15});centered=true; }
  };
  window.citsFocus = key => { const m=markers.get(key);if(m){map.setView(m.getLatLng(),17);m.openPopup();} };
  window.citsFit = () => {if(markers.size)map.fitBounds(L.featureGroup([...markers.values()]).getBounds().pad(.3),{maxZoom:16});};
  new ResizeObserver(()=>map.invalidateSize()).observe(document.getElementById('traffic-map'));
  if (window.__citsData) window.citsRender(window.__citsData);
})();
