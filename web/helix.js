import * as THREE from 'three';

export async function createHelix(container) {
  const renderer = new THREE.WebGLRenderer({antialias:true,alpha:true});
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.5));
  renderer.setClearColor(0x000000,0);
  container.appendChild(renderer.domElement);
  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(40,1,.1,100);
  camera.position.set(0,0,16);
  const root = new THREE.Group();
  root.rotation.z = -.16;
  scene.add(root);
  const sphere = new THREE.SphereGeometry(.115,12,10);
  const cyan = new THREE.MeshStandardMaterial({color:0x22d3ee,emissive:0x0a5267,emissiveIntensity:.65});
  const magenta = new THREE.MeshStandardMaterial({color:0xe64ab0,emissive:0x4d123b,emissiveIntensity:.65});
  const rungMat = new THREE.MeshStandardMaterial({color:0x4fa0b4,emissive:0x154a57,emissiveIntensity:.3});
  const count = 84;
  const first = new THREE.InstancedMesh(sphere,cyan,count);
  const second = new THREE.InstancedMesh(sphere,magenta,count);
  const transform = new THREE.Object3D();
  const point = index => {
    const y = (index / (count-1)-.5)*12;
    const angle = index * .22;
    return [new THREE.Vector3(Math.cos(angle)*1.85,y,Math.sin(angle)*1.85),new THREE.Vector3(Math.cos(angle+Math.PI)*1.85,y,Math.sin(angle+Math.PI)*1.85)];
  };
  for(let i=0;i<count;i++) {
    const [a,b] = point(i);
    transform.position.copy(a); transform.updateMatrix(); first.setMatrixAt(i,transform.matrix);
    transform.position.copy(b); transform.updateMatrix(); second.setMatrixAt(i,transform.matrix);
    if(i%2===0) {
      const direction=b.clone().sub(a);
      const geometry=new THREE.CylinderGeometry(.025,.025,direction.length(),6);
      const rung=new THREE.Mesh(geometry,rungMat);
      rung.position.copy(a).add(b).multiplyScalar(.5);
      rung.quaternion.setFromUnitVectors(new THREE.Vector3(0,1,0),direction.normalize());
      root.add(rung);
    }
  }
  root.add(first,second);
  scene.add(new THREE.AmbientLight(0x8cbac9,1.1));
  const light = new THREE.PointLight(0xe1ffff,85,50);
  light.position.set(4,4,8); scene.add(light);
  const edge = new THREE.PointLight(0xe64ab0,55,35);
  edge.position.set(-5,-4,5); scene.add(edge);
  const ring = new THREE.Mesh(new THREE.TorusGeometry(2.28,.035,8,120),new THREE.MeshBasicMaterial({color:0x22d3ee,transparent:true,opacity:.9}));
  ring.rotation.x=Math.PI/2; ring.position.y=-6;
  root.add(ring);
  const halo = new THREE.Mesh(new THREE.TorusGeometry(2.33,.15,8,100),new THREE.MeshBasicMaterial({color:0x22d3ee,transparent:true,opacity:.09}));
  halo.rotation.x=Math.PI/2; root.add(halo);
  let fraction=0;
  const resize = () => {
    const width=container.clientWidth, height=container.clientHeight;
    renderer.setSize(width,height,false);
    camera.aspect=width / Math.max(1,height);
    camera.updateProjectionMatrix();
  };
  const observer = new ResizeObserver(resize); observer.observe(container); resize();
  let stopped=false;
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  renderer.setAnimationLoop(() => {
    if(stopped) return;
    if(!motion.matches)root.rotation.y += .002;
    ring.position.y=-6+fraction*12; halo.position.y=ring.position.y;
    renderer.render(scene,camera);
  });
  const dispose=()=>{stopped=true;renderer.setAnimationLoop(null);observer.disconnect();scene.traverse(object=>{if(object.geometry)object.geometry.dispose();if(object.material){if(Array.isArray(object.material))object.material.forEach(material=>material.dispose());else object.material.dispose()}});renderer.dispose();};
  window.disposeHelix = dispose;
  return {
    update(value, hits) { fraction=Math.min(Math.max(value,0),1); ring.material.color.set(hits?0xf5a524:0x22d3ee); },
    setHit() {},
  };
}
export function disposeHelix() { if(window.disposeHelix)window.disposeHelix(); }
