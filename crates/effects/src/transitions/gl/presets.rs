//! GL Transition's factory presets: the gl-transitions collection's code
//! (MIT; its authors and license in
//! `crates/effects/licenses/gl-transitions.md`), its uniforms the panel's
//! controls (`kParamFloat0`…) with the defaults and slider ranges their
//! comments give, in the reference's list's order.

/// A preset: its name, its settings (the preset's number first) and its
/// code.
pub(super) struct GlPreset {
    pub(super) name: &'static str,
    pub(super) values: &'static [(&'static str, &'static str)],
    pub(super) code: &'static str,
}

pub(super) const PRESETS: &[GlPreset] = &[
    GlPreset {
        name: "Bounce",
        values: &[
            ("IsPreset", "14"),
            ("kParamFloat0", "0.075"),
            ("kParamFloat1", "3"),
            ("kParamVec40", "0 0 0 0.6"),
            ("kParamFloat0Name", "Shadow Height"),
            ("kParamFloat1Name", "Bounces"),
            ("kParamVec40Name", "Shadow Color"),
        ],
        code: "// Author: Adrian Purser
// License: MIT

uniform vec4 kParamVec40; // = vec4(0.,0.,0.,.6)
uniform float kParamFloat0; // = 0.075; [0.01,0.4]
uniform float kParamFloat1; // = 3.0 [0.1, 10]

const float PI = 3.14159265358;

vec4 transition (vec2 uv) {
  float time = progress;
  float stime = sin(time * PI / 2.);
  float phase = time * PI * kParamFloat1;
  float y = (abs(cos(phase))) * (1.0 - stime);
  float d = uv.y - y;
  return mix(
    mix(
      getToColor(uv),
      kParamVec40,
      step(d, kParamFloat0) * (1. - mix(
        ((d / kParamFloat0) * kParamVec40.a) + (1.0 - kParamVec40.a),
        1.0,
        smoothstep(0.95, 1., progress) // fade-out the shadow at the end
      ))
    ),
    getFromColor(vec2(uv.x, uv.y + (1.0 - y))),
    step(d, 0.0)
  );
}
",
    },
    GlPreset {
        name: "Bow Tie Horizontal",
        values: &[
            ("IsPreset", "2"),
        ],
        code: "// Author: huynx
// License: MIT

vec2 bottom_left = vec2(0.0, 1.0);
vec2 bottom_right = vec2(1.0, 1.0);
vec2 top_left = vec2(0.0, 0.0);
vec2 top_right = vec2(1.0, 0.0);
vec2 center = vec2(0.5, 0.5);

float check(vec2 p1, vec2 p2, vec2 p3)
{
  return (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y);
}

bool PointInTriangle (vec2 pt, vec2 p1, vec2 p2, vec2 p3)
{
    bool b1, b2, b3;
    b1 = check(pt, p1, p2) < 0.0;
    b2 = check(pt, p2, p3) < 0.0;
    b3 = check(pt, p3, p1) < 0.0;
    return ((b1 == b2) && (b2 == b3));
}

bool in_left_triangle(vec2 p){
  vec2 vertex1, vertex2, vertex3;
  vertex1 = vec2(progress, 0.5);
  vertex2 = vec2(0.0, 0.5-progress);
  vertex3 = vec2(0.0, 0.5+progress);
  if (PointInTriangle(p, vertex1, vertex2, vertex3))
  {
    return true;
  }
  return false;
}

bool in_right_triangle(vec2 p){
  vec2 vertex1, vertex2, vertex3;
  vertex1 = vec2(1.0-progress, 0.5);
  vertex2 = vec2(1.0, 0.5-progress);
  vertex3 = vec2(1.0, 0.5+progress);
  if (PointInTriangle(p, vertex1, vertex2, vertex3))
  {
    return true;
  }
  return false;
}

float blur_edge(vec2 bot1, vec2 bot2, vec2 top, vec2 testPt)
{
  vec2 lineDir = bot1 - top;
  vec2 perpDir = vec2(lineDir.y, -lineDir.x);
  vec2 dirToPt1 = bot1 - testPt;
  float dist1 = abs(dot(normalize(perpDir), dirToPt1));
  
  lineDir = bot2 - top;
  perpDir = vec2(lineDir.y, -lineDir.x);
  dirToPt1 = bot2 - testPt;
  float min_dist = min(abs(dot(normalize(perpDir), dirToPt1)), dist1);
  
  if (min_dist < 0.005) {
    return min_dist / 0.005;
  }
  else  {
    return 1.0;
  };
}


vec4 transition (vec2 uv) {
  if (in_left_triangle(uv))
  {
    if (progress < 0.1)
    {
      return getFromColor(uv);
    }
    if (uv.x < 0.5)
    {
      vec2 vertex1 = vec2(progress, 0.5);
      vec2 vertex2 = vec2(0.0, 0.5-progress);
      vec2 vertex3 = vec2(0.0, 0.5+progress);
      return mix(
        getFromColor(uv),
        getToColor(uv),
        blur_edge(vertex2, vertex3, vertex1, uv)
      );
    }
    else
    {
      if (progress > 0.0)
      {
        return getToColor(uv);
      }
      else
      {
        return getFromColor(uv);
      }
    }    
  }
  else if (in_right_triangle(uv))
  {
    if (uv.x >= 0.5)
    {
      vec2 vertex1 = vec2(1.0-progress, 0.5);
      vec2 vertex2 = vec2(1.0, 0.5-progress);
      vec2 vertex3 = vec2(1.0, 0.5+progress);
      return mix(
        getFromColor(uv),
        getToColor(uv),
        blur_edge(vertex2, vertex3, vertex1, uv)
      );  
    }
    else
    {
      return getFromColor(uv);
    }
  }
  else {
    return getFromColor(uv);
  }
}",
    },
    GlPreset {
        name: "Bow Tie Vertical",
        values: &[
            ("IsPreset", "1"),
        ],
        code: "// Author: huynx
// License: MIT

float check(vec2 p1, vec2 p2, vec2 p3)
{
  return (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y);
}

bool PointInTriangle (vec2 pt, vec2 p1, vec2 p2, vec2 p3)
{
    bool b1, b2, b3;
    b1 = check(pt, p1, p2) < 0.0;
    b2 = check(pt, p2, p3) < 0.0;
    b3 = check(pt, p3, p1) < 0.0;
    return ((b1 == b2) && (b2 == b3));
}

bool in_top_triangle(vec2 p){
  vec2 vertex1, vertex2, vertex3;
  vertex1 = vec2(0.5, progress);
  vertex2 = vec2(0.5-progress, 0.0);
  vertex3 = vec2(0.5+progress, 0.0);
  if (PointInTriangle(p, vertex1, vertex2, vertex3))
  {
    return true;
  }
  return false;
}

bool in_bottom_triangle(vec2 p){
  vec2 vertex1, vertex2, vertex3;
  vertex1 = vec2(0.5, 1.0 - progress);
  vertex2 = vec2(0.5-progress, 1.0);
  vertex3 = vec2(0.5+progress, 1.0);
  if (PointInTriangle(p, vertex1, vertex2, vertex3))
  {
    return true;
  }
  return false;
}

float blur_edge(vec2 bot1, vec2 bot2, vec2 top, vec2 testPt)
{
  vec2 lineDir = bot1 - top;
  vec2 perpDir = vec2(lineDir.y, -lineDir.x);
  vec2 dirToPt1 = bot1 - testPt;
  float dist1 = abs(dot(normalize(perpDir), dirToPt1));
  
  lineDir = bot2 - top;
  perpDir = vec2(lineDir.y, -lineDir.x);
  dirToPt1 = bot2 - testPt;
  float min_dist = min(abs(dot(normalize(perpDir), dirToPt1)), dist1);
  
  if (min_dist < 0.005) {
    return min_dist / 0.005;
  }
  else  {
    return 1.0;
  };
}


vec4 transition (vec2 uv) {
  if (in_top_triangle(uv))
  {
    if (progress < 0.1)
    {
      return getFromColor(uv);
    }
    if (uv.y < 0.5)
    {
      vec2 vertex1 = vec2(0.5, progress);
      vec2 vertex2 = vec2(0.5-progress, 0.0);
      vec2 vertex3 = vec2(0.5+progress, 0.0);
      return mix(
        getFromColor(uv),
        getToColor(uv),
        blur_edge(vertex2, vertex3, vertex1, uv)
      );
    }
    else
    {
      if (progress > 0.0)
      {
        return getToColor(uv);
      }
      else
      {
        return getFromColor(uv);
      }
    }    
  }
  else if (in_bottom_triangle(uv))
  {
    if (uv.y >= 0.5)
    {
      vec2 vertex1 = vec2(0.5, 1.0-progress);
      vec2 vertex2 = vec2(0.5-progress, 1.0);
      vec2 vertex3 = vec2(0.5+progress, 1.0);
      return mix(
        getFromColor(uv),
        getToColor(uv),
        blur_edge(vertex2, vertex3, vertex1, uv)
      );  
    }
    else
    {
      return getFromColor(uv);
    }
  }
  else {
    return getFromColor(uv);
  }
}",
    },
    GlPreset {
        name: "Burn",
        values: &[
            ("IsPreset", "36"),
        ],
        code: "// author: gre
// License: MIT
const vec3 color = vec3(0.9, 0.4, 0.2);
vec4 transition (vec2 uv) {
  return mix(
    getFromColor(uv) + vec4(progress*color, 1.0),
    getToColor(uv) + vec4((1.0-progress)*color, 1.0),
    progress
  );
}
",
    },
    GlPreset {
        name: "Butterfly Wave Scrawler",
        values: &[
            ("IsPreset", "27"),
            ("kParamFloat0", "1"),
            ("kParamFloat1", "13"),
            ("kParamFloat2", "0.3"),
            ("kParamFloat0Name", "Amplitude"),
            ("kParamFloat1Name", "Waves"),
            ("kParamFloat2Name", "Color Separation"),
        ],
        code: "// Author: mandubian
// License: MIT
uniform float kParamFloat0; // = 1.0 [0.1,3]
uniform float kParamFloat1; // = 13.0 [2,15]
uniform float kParamFloat2; // = 0.3 [0.1,0.5]
float PI = 3.14159265358979323846264;
float compute(vec2 p, float progress, vec2 center) {
vec2 o = p*sin(progress * kParamFloat0)-center;
// horizontal vector
vec2 h = vec2(1., 0.);
// butterfly polar function (don't ask me why this one :))
float theta = acos(dot(o, h)) * kParamFloat1;
return (exp(cos(theta)) - 2.*cos(4.*theta) + pow(sin((2.*theta - PI) / 24.), 5.)) / 10.;
}
vec4 transition(vec2 uv) {
  vec2 p = uv.xy / vec2(1.0).xy;
  float inv = 1. - progress;
  vec2 dir = p - vec2(.5);
  float dist = length(dir);
  float disp = compute(p, progress, vec2(0.5, 0.5)) ;
  vec4 texTo = getToColor(p + inv*disp);
  vec4 texFrom = vec4(
  getFromColor(p + progress*disp*(1.0 - kParamFloat2)).r,
  getFromColor(p + progress*disp).g,
  getFromColor(p + progress*disp*(1.0 + kParamFloat2)).b,
  1.0);
  return texTo*progress + texFrom*inv;
}
",
    },
    GlPreset {
        name: "Circle Crop",
        values: &[
            ("IsPreset", "17"),
            ("kParamVec40", "0 0 0 1.0"),
            ("kParamVec40Name", "Background"),
        ],
        code: "// License: MIT
// Author: fkuteken
// ported by gre from https://gist.github.com/fkuteken/f63e3009c1143950dee9063c3b83fb88

uniform vec4 kParamVec40; // = vec4(0.0, 0.0, 0.0, 1.0)

vec2 ratio2 = vec2(1.0, 1.0 / ratio);
float s = pow(2.0 * abs(progress - 0.5), 3.0);

vec4 transition(vec2 p) {
  float dist = length((vec2(p) - 0.5) * ratio2);
  return mix(
    progress < 0.5 ? getFromColor(p) : getToColor(p), // branching is ok here as we statically depend on progress uniform (branching won't change over pixels)
    kParamVec40,
    step(s, dist)
  );
}
",
    },
    GlPreset {
        name: "Circle Open",
        values: &[
            ("IsPreset", "37"),
            ("kParamFloat0", "0.3"),
            ("kParamFloat0Name", "Smoothness"),
        ],
        code: "// author: gre
// License: MIT
uniform float kParamFloat0; // = 0.3; [0,1.5]
const bool opening = true;

const vec2 center = vec2(0.5, 0.5);
const float SQRT_2 = 1.414213562373;

vec4 transition (vec2 uv) {
  float x = opening ? progress : 1.-progress;
  float m = smoothstep(-kParamFloat0, 0.0, SQRT_2*distance(center, uv) - x*(1.+kParamFloat0));
  return mix(getFromColor(uv), getToColor(uv), opening ? 1.-m : m);
}
",
    },
    GlPreset {
        name: "Color Distance",
        values: &[
            ("IsPreset", "16"),
            ("kParamFloat0", "5"),
            ("kParamFloat0Name", "Power"),
        ],
        code: "// License: MIT
// Author: P-Seebauer
// ported by gre from https://gist.github.com/P-Seebauer/2a5fa2f77c883dd661f9

uniform float kParamFloat0; // = 5.0; [0.5,20]

vec4 transition(vec2 p) {
  vec4 fTex = getFromColor(p);
  vec4 tTex = getToColor(p);
  float m = step(distance(fTex, tTex), progress);
  return mix(
    mix(fTex, tTex, m),
    tTex,
    pow(progress, kParamFloat0)
  );
}
",
    },
    GlPreset {
        name: "Color Fade",
        values: &[
            ("IsPreset", "42"),
            ("kParamVec40", "0 0 0 1.0"),
            ("kParamFloat0", "0.4"),
            ("kParamVec40Name", "Color"),
            ("kParamFloat0Name", "Color Phase"),
        ],
        code: "// Author: gre
// License: MIT
// Description: A fade through a solid kParamVec40, black by default
// Tags: fade, kParamVec40
// @kParamVec40 Color to fade through
uniform vec4 kParamVec40;// = vec4(0,0,0,1)
// @range(0, 1) How long the kParamVec40 shows in the middle
uniform float kParamFloat0;// = 0.4; [0.1,0.9]   // if 0.0, there is no black phase, if 0.9, the black phase is very important
vec4 transition (vec2 uv) {
  return mix(
    mix(kParamVec40, getFromColor(uv), smoothstep(1.0-kParamFloat0, 0.0, progress)),
    mix(kParamVec40, getToColor(uv), smoothstep(    kParamFloat0, 1.0, progress)),
    progress);
}
",
    },
    GlPreset {
        name: "Color Phase",
        values: &[
            ("IsPreset", "38"),
            ("kParamVec40", "0.0 0.2 0.4 0.0"),
            ("kParamVec41", "0.6 0.8 1.0 1.0"),
            ("kParamVec40Name", "from"),
            ("kParamVec41Name", "to"),
        ],
        code: "// Author: gre
// License: MIT

// Usage: kParamVec40 and kParamVec41 must be in [0.0, 1.0] range 
// and all(kParamVec40) must be < all(kParamVec41)

uniform vec4 kParamVec40; // = vec4(0.0, 0.2, 0.4, 0.0)
uniform vec4 kParamVec41; // = vec4(0.6, 0.8, 1.0, 1.0)

vec4 transition (vec2 uv) {
  vec4 a = getFromColor(uv);
  vec4 b = getToColor(uv);
  return mix(a, b, smoothstep(kParamVec40, kParamVec41, vec4(progress)));
}
",
    },
    GlPreset {
        name: "Color Planes",
        values: &[
            ("IsPreset", "59"),
            ("kParamFloat0", "0.5"),
            ("kParamFloat1", "0.2"),
            ("kParamFloat0Name", "Opacity"),
            ("kParamFloat1Name", "Width"),
        ],
        code: "// Author:Adam Liu
// License: MIT

uniform float kParamFloat0;// = 0.5; [0,1]
uniform float kParamFloat1;// = 0.2; [0.01,1]


const vec2 boundMin = vec2(0, 0);
const vec2 boundMax = vec2(1.0, 1.0);
const vec4 black = vec4(0, 0, 0, 1.0);
const float pi = 3.1415926;

bool inBounds (vec2 p) {
  return all(lessThan(boundMin, p)) && all(lessThan(p, boundMax));
}

mat2 transitionMatrix(float p)
{
    float degree = pi*p;
    return 1./cos(degree) * mat2(vec2(1., 0),
                                vec2(0, cos(degree)));
}

vec2 transitionCoord(vec2 coordinate, mat2 matrix, float offset, float p)
{
    vec2 center = vec2(coordinate.x - 0.5, coordinate.y - 0.5);
    
    vec2 coor = matrix * vec2(center.x + sin(pi*progress)*offset, center.y);
    coor = vec2(coor.x + 0.5 - sin(pi*p)*offset, coor.y+0.5);
    
    return coor;
}


vec4 transition (vec2 uv) {
  
    vec4 redPlane, greenPlane, bluePlane;
     
    mat2 matrix = transitionMatrix(progress);
     
    vec2 redCoord = transitionCoord(uv, matrix, kParamFloat1, progress);
    
    vec2 greenCoord = transitionCoord(uv, matrix, 0., progress);
    vec2 blueCoord = transitionCoord(uv, matrix, -kParamFloat1, progress);
    
    if (progress>0.5) {
        matrix = transitionMatrix(1.0-progress);
    }
    
   
    if (!inBounds(redCoord)) {
        redPlane = black;
    } else if (progress <= 0.5) {
        vec4 src = getFromColor(redCoord);
    } else {
        vec4 des = getToColor(vec2(1.-redCoord.x, redCoord.y));
        redPlane = vec4(des.r, 0.0, 0.0, des.a);
    }
     
   
    if (!inBounds(greenCoord)) {
        greenPlane = black;
    } else if (progress <= 0.5) {
        vec4 src = getFromColor(greenCoord);
        greenPlane = vec4(0, src.g, 0, src.a);
    } else {
        vec4 des = getToColor(vec2(1.-greenCoord.x, greenCoord.y));
        greenPlane = vec4(0, des.g, 0, des.a);
    }
   
    if (!inBounds(blueCoord)) {
        bluePlane = black;
    } else if (progress <= 0.5) {
        vec4 src = getFromColor(blueCoord);
        bluePlane = vec4(0, 0, src.b, src.a);
    } else {
        vec4 des = getToColor(vec2(1.-blueCoord.x, blueCoord.y));;
        bluePlane = vec4(0, 0, des.b, des.a);
    }
   
    if (progress == 1.) {
        return getToColor(uv);
    } else if (progress == 0.0) {
        return getFromColor(uv);
    } else {
        return mix(mix(redPlane, greenPlane, kParamFloat0), bluePlane, kParamFloat0);
    }
}",
    },
    GlPreset {
        name: "Crazy Parametric Fun",
        values: &[
            ("IsPreset", "26"),
            ("kParamFloat0", "4"),
            ("kParamFloat1", "120"),
            ("kParamFloat2", "0.1"),
            ("kParamFloat0Name", "Wavelength"),
            ("kParamFloat1Name", "Amplitude"),
            ("kParamFloat2Name", "Smoothness"),
        ],
        code: "// Author: mandubian
// License: MIT

uniform float kParamFloat0; // = 4; [1,10]
const float b = 1.0;
uniform float kParamFloat1; // = 120; [1,300]
uniform float kParamFloat2; // = 0.1; [0.01,2]

vec4 transition(vec2 uv) {
  vec2 p = uv.xy / vec2(1.0).xy;
  vec2 dir = p - vec2(.5);
  float dist = length(dir);
  float x = (kParamFloat0 - b) * cos(progress) + b * cos(progress * ((kParamFloat0 / b) - 1.) );
  float y = (kParamFloat0 - b) * sin(progress) - b * sin(progress * ((kParamFloat0 / b) - 1.));
  vec2 offset = dir * vec2(sin(progress  * dist * kParamFloat1 * x), sin(progress * dist * kParamFloat1 * y)) / kParamFloat2;
  return mix(getFromColor(p + offset), getToColor(p), smoothstep(0.2, 1.0, progress));
}
",
    },
    GlPreset {
        name: "Cross Hatch",
        values: &[
            ("IsPreset", "25"),
            ("kParamVec20", "0.5 0.5"),
            ("kParamFloat0", "3"),
            ("kParamFloat1", "0.1"),
            ("kParamVec20Name", "Center"),
            ("kParamFloat0Name", "Threshold"),
            ("kParamFloat1Name", "Fade Edge"),
        ],
        code: "// License: MIT
// Author: pthrasher
// adapted by gre from https://gist.github.com/pthrasher/04fd9a7de4012cbb03f6

uniform vec2 kParamVec20; // = vec2(0.5) [0,0,1,1]
uniform float kParamFloat0; // = 3.0  [0,6]
uniform float kParamFloat1; // = 0.1  [0.0,0.9]

float rand(vec2 co) {
  return fract(sin(dot(co.xy ,vec2(12.9898,78.233))) * 43758.5453);
}
vec4 transition(vec2 p) {
  float dist = distance(kParamVec20, p) / kParamFloat0;
  float r = progress - min(rand(vec2(p.y, 0.0)), rand(vec2(0.0, p.x)));
  return mix(getFromColor(p), getToColor(p), mix(0.0, mix(step(dist, r), 1.0, smoothstep(1.0-kParamFloat1, 1.0, progress)), smoothstep(0.0, kParamFloat1, progress)));    
}
",
    },
    GlPreset {
        name: "Cross Warp",
        values: &[
            ("IsPreset", "39"),
        ],
        code: "// Author: Eke Péter <peterekepeter@gmail.com>
// License: MIT
vec4 transition(vec2 p) {
  float x = progress;
  x=smoothstep(.0,1.0,(x*2.0+p.x-1.0));
  return mix(getFromColor((p-.5)*(1.-x)+.5), getToColor((p-.5)*x+.5), x);
}
",
    },
    GlPreset {
        name: "Cross Zoom",
        values: &[
            ("IsPreset", "19"),
            ("kParamFloat0", "0.4"),
            ("kParamFloat0Name", "Strength"),
        ],
        code: "// License: MIT
// Author: rectalogic
// ported by gre from https://gist.github.com/rectalogic/b86b90161503a0023231

// Converted from https://github.com/rectalogic/rendermix-basic-effects/blob/master/assets/com/rendermix/CrossZoom/CrossZoom.frag
// Which is based on https://github.com/evanw/glfx.js/blob/master/src/filters/blur/zoomblur.js
// With additional easing functions from https://github.com/rectalogic/rendermix-basic-effects/blob/master/assets/com/rendermix/Easing/Easing.glsllib

uniform float kParamFloat0; // = 0.4; [0.1,1]

const float PI = 3.141592653589793;

float Linear_ease(in float begin, in float change, in float duration, in float time) {
    return change * time / duration + begin;
}

float Exponential_easeInOut(in float begin, in float change, in float duration, in float time) {
    if (time == 0.0)
        return begin;
    else if (time == duration)
        return begin + change;
    time = time / (duration / 2.0);
    if (time < 1.0)
        return change / 2.0 * pow(2.0, 10.0 * (time - 1.0)) + begin;
    return change / 2.0 * (-pow(2.0, -10.0 * (time - 1.0)) + 2.0) + begin;
}

float Sinusoidal_easeInOut(in float begin, in float change, in float duration, in float time) {
    return -change / 2.0 * (cos(PI * time / duration) - 1.0) + begin;
}

float rand (vec2 co) {
  return fract(sin(dot(co.xy ,vec2(12.9898,78.233))) * 43758.5453);
}

vec3 crossFade(in vec2 uv, in float dissolve) {
    return mix(getFromColor(uv).rgb, getToColor(uv).rgb, dissolve);
}

vec4 transition(vec2 uv) {
    vec2 texCoord = uv.xy / vec2(1.0).xy;

    // Linear interpolate center across center half of the image
    vec2 center = vec2(Linear_ease(0.25, 0.5, 1.0, progress), 0.5);
    float dissolve = Exponential_easeInOut(0.0, 1.0, 1.0, progress);

    // Mirrored sinusoidal loop. 0->kParamFloat0 then kParamFloat0->0
    float kParamFloat0 = Sinusoidal_easeInOut(0.0, kParamFloat0, 0.5, progress);

    vec3 color = vec3(0.0);
    float total = 0.0;
    vec2 toCenter = center - texCoord;

    /* randomize the lookup values to hide the fixed number of samples */
    float offset = rand(uv);

    for (float t = 0.0; t <= 40.0; t++) {
        float percent = (t + offset) / 40.0;
        float weight = 4.0 * (percent - percent * percent);
        color += crossFade(texCoord + toCenter * percent * kParamFloat0, dissolve) * weight;
        total += weight;
    }
    return vec4(color / total, 1.0);
}
",
    },
    GlPreset {
        name: "Cube",
        values: &[
            ("IsPreset", "40"),
            ("kParamFloat0", "0.7"),
            ("kParamFloat1", "0.3"),
            ("kParamFloat2", "0.4"),
            ("kParamFloat3", "3"),
            ("kParamFloat0Name", "Perspective"),
            ("kParamFloat1Name", "Unzoom"),
            ("kParamFloat2Name", "Reflection"),
            ("kParamFloat3Name", "Floating"),
        ],
        code: "// Author: gre
// License: MIT
uniform float kParamFloat0; // = 0.7; [0,1]
uniform float kParamFloat1; // = 0.3; [0,1]
uniform float kParamFloat2; // = 0.4; [0,1]
uniform float kParamFloat3; // = 3.0; [2,4]

vec2 project (vec2 p) {
  return p * vec2(1.0, -1.2) + vec2(0.0, -kParamFloat3/100.);
}

bool inBounds (vec2 p) {
  return all(lessThan(vec2(0.0), p)) && all(lessThan(p, vec2(1.0)));
}

vec4 bgColor (vec2 p, vec2 pfr, vec2 pto) {
  vec4 c = vec4(0.0, 0.0, 0.0, 1.0);
  pfr = project(pfr);
  // FIXME avoid branching might help perf!
  if (inBounds(pfr)) {
    c += mix(vec4(0.0), getFromColor(pfr), kParamFloat2 * mix(1.0, 0.0, pfr.y));
  }
  pto = project(pto);
  if (inBounds(pto)) {
    c += mix(vec4(0.0), getToColor(pto), kParamFloat2 * mix(1.0, 0.0, pto.y));
  }
  return c;
}

// p : the position
// kParamFloat0 : the perspective in [ 0, 1 ]
// center : the xcenter in [0, 1] \\ 0.5 excluded
vec2 xskew (vec2 p, float kParamFloat0, float center) {
  float x = mix(p.x, 1.0-p.x, center);
  return (
    (
      vec2( x, (p.y - 0.5*(1.0-kParamFloat0) * x) / (1.0+(kParamFloat0-1.0)*x) )
      - vec2(0.5-distance(center, 0.5), 0.0)
    )
    * vec2(0.5 / distance(center, 0.5) * (center<0.5 ? 1.0 : -1.0), 1.0)
    + vec2(center<0.5 ? 0.0 : 1.0, 0.0)
  );
}

vec4 transition(vec2 op) {
  float uz = kParamFloat1 * 2.0*(0.5-distance(0.5, progress));
  vec2 p = -uz*0.5+(1.0+uz) * op;
  vec2 fromP = xskew(
    (p - vec2(progress, 0.0)) / vec2(1.0-progress, 1.0),
    1.0-mix(progress, 0.0, kParamFloat0),
    0.0
  );
  vec2 toP = xskew(
    p / vec2(progress, 1.0),
    mix(pow(progress, 2.0), 1.0, kParamFloat0),
    1.0
  );
  // FIXME avoid branching might help perf!
  if (inBounds(fromP)) {
    return getFromColor(fromP);
  }
  else if (inBounds(toP)) {
    return getToColor(toP);
  }
  return bgColor(op, fromP, toP);
}
",
    },
    GlPreset {
        name: "Directional",
        values: &[
            ("IsPreset", "3"),
            ("kParamVec20", "0 0.5"),
            ("kParamVec20Name", "Direction"),
        ],
        code: "// Author: Gaëtan Renaudeau
// License: MIT

uniform vec2 kParamVec20; // = vec2(0.0, 0.5) [-1,-1,1,1]

vec4 transition (vec2 uv) {
  vec2 p = uv + progress * sign(kParamVec20);
  vec2 f = fract(p);
  return mix(
    getToColor(f),
    getFromColor(f),
    step(0.0, p.y) * step(p.y, 1.0) * step(0.0, p.x) * step(p.x, 1.0)
  );
}
",
    },
    GlPreset {
        name: "Directional Scale",
        values: &[
            ("IsPreset", "60"),
            ("kParamFloat0", "0.7"),
            ("kParamVec20", "0 1"),
            ("kParamFloat0Name", "Scale"),
            ("kParamVec20Name", "Direction"),
        ],
        code: "// Author: Thibaut Foussard
// based on Directional transition by Gaëtan Renaudeau
// https://gl-transitions.com/editor/Directional
// License: MIT
// Description: Like Directional, with both images scaled down while they slide
// Tags: slide, push, zoom, directional

#define PI acos(-1.0)

// @param Direction of the movement
uniform vec2 kParamVec20; // = vec2(0.0, 1.0)
uniform float kParamFloat0; // = 0.7; [0.01,1.1]

float parabola(float x) {
  float y = pow(sin(x * PI), 1.);
  return y;
}

vec4 transition (vec2 uv) {
  float easedProgress = pow(sin(progress  * PI / 2.), 3.);
  vec2 p = uv + easedProgress * sign(kParamVec20);
  vec2 f = fract(p);
  
  float s = 1. - (1. - (1. / kParamFloat0)) * parabola(progress);
  f = (f - 0.5) * s  + 0.5;
  
  float mixer = step(0.0, p.y) * step(p.y, 1.0) * step(0.0, p.x) * step(p.x, 1.0);
  vec4 col = mix(getToColor(f), getFromColor(f), mixer);
  
  float border = step(0., f.x) * step(0., (1. - f.x)) * step(0., f.y) * step(0., 1. - f.y);
  col *= border;
  
  return col;
}
",
    },
    GlPreset {
        name: "Directional Warp",
        values: &[
            ("IsPreset", "13"),
            ("kParamFloat0", "0.5"),
            ("kParamVec20", "-1 1"),
            ("kParamFloat0Name", "Smoothness"),
            ("kParamVec20Name", "Direction"),
        ],
        code: "// Author: pschroen
// License: MIT
// Description: A diagonal wipe with a warped, smeared edge
// Tags: wipe, warp, directional

// @param Softness of the edge
uniform float kParamFloat0; // = 0.5; [0.0,1.0]
// @param Direction of the wipe
uniform vec2 kParamVec20; // = vec2(-1.0, 1.0) [-1,-1,1,1]

const vec2 center = vec2(0.5, 0.5);

vec4 transition (vec2 uv) {
  vec2 v = normalize(kParamVec20);
  v /= abs(v.x) + abs(v.y);
  float d = v.x * center.x + v.y * center.y;
  float m = 1.0 - smoothstep(-kParamFloat0, 0.0, v.x * uv.x + v.y * uv.y - (d - 0.5 + progress * (1.0 + kParamFloat0)));
  return mix(getFromColor((uv - 0.5) * (1.0 - m) + 0.5), getToColor((uv - 0.5) * m + 0.5), m);
}
",
    },
    GlPreset {
        name: "Doom Screen",
        values: &[
            ("IsPreset", "33"),
            ("kParamInt0", "30"),
            ("kParamFloat1", "2"),
            ("kParamFloat2", "0.5"),
            ("kParamInt0Name", "Bars"),
            ("kParamFloat1Name", "Amplitude"),
            ("kParamFloat2Name", "Drip Scale"),
        ],
        code: "// Author: Zeh Fernando
// License: MIT


// Transition parameters --------

// Number of total kParamInt0/columns
uniform int kParamInt0; // = 30; [2,100]

// Multiplier for speed ratio. 0 = no variation when going down, higher = some elements go much faster
uniform float kParamFloat1; // = 2; [0,10]

// Further variations in speed. 0 = no noise, 1 = super noisy (ignore frequency)
const float noise = 0.1;

// Speed variation horizontally. the bigger the value, the shorter the waves
const float frequency = 0.5;

// How much the kParamInt0 seem to \"run\" from the middle of the screen first (sticking to the sides). 0 = no drip, 1 = curved drip
uniform float kParamFloat2; // = 0.5 [0,1]


// The code proper --------

float rand(int num) {
  return fract(mod(float(num) * 67123.313, 12.0) * sin(float(num) * 10.3) * cos(float(num)));
}

float wave(int num) {
  float fn = float(num) * frequency * 0.1 * float(kParamInt0);
  return cos(fn * 0.5) * cos(fn * 0.13) * sin((fn+10.0) * 0.3) / 2.0 + 0.5;
}

float drip(int num) {
  return sin(float(num) / float(kParamInt0 - 1) * 3.141592) * kParamFloat2;
}

float pos(int num) {
  return (noise == 0.0 ? wave(num) : mix(wave(num), rand(num), noise)) + (kParamFloat2 == 0.0 ? 0.0 : drip(num));
}

vec4 transition(vec2 uv) {
  int bar = int(uv.x * (float(kParamInt0)));
  float scale = 1.0 + pos(bar) * kParamFloat1;
  float phase = progress * scale;
  float posY = uv.y / vec2(1.0).y;
  vec2 p;
  vec4 c;
  if (phase + posY < 1.0) {
    p = vec2(uv.x, uv.y + mix(0.0, vec2(1.0).y, phase)) / vec2(1.0).xy;
    c = getFromColor(p);
  } else {
    p = uv.xy / vec2(1.0).xy;
    c = getToColor(p);
  }

  // Finally, apply the color
  return c;
}
",
    },
    GlPreset {
        name: "Doorway",
        values: &[
            ("IsPreset", "41"),
            ("kParamFloat0", "0.4"),
            ("kParamFloat1", "0.4"),
            ("kParamFloat2", "3"),
            ("kParamFloat0Name", "Reflection"),
            ("kParamFloat1Name", "Perspective"),
            ("kParamFloat2Name", "Depth"),
        ],
        code: "// author: gre
// License: MIT 
uniform float kParamFloat0; // = 0.4; [0,0.8]
uniform float kParamFloat1; // = 0.4; [0.1;1]
uniform float kParamFloat2; // = 3;  [0,6]

const vec4 black = vec4(0.0, 0.0, 0.0, 1.0);
const vec2 boundMin = vec2(0.0, 0.0);
const vec2 boundMax = vec2(1.0, 1.0);

bool inBounds (vec2 p) {
  return all(lessThan(boundMin, p)) && all(lessThan(p, boundMax));
}

vec2 project (vec2 p) {
  return p * vec2(1.0, -1.2) + vec2(0.0, -0.02);
}

vec4 bgColor (vec2 p, vec2 pto) {
  vec4 c = black;
  pto = project(pto);
  if (inBounds(pto)) {
    c += mix(black, getToColor(pto), kParamFloat0 * mix(1.0, 0.0, pto.y));
  }
  return c;
}


vec4 transition (vec2 p) {
  vec2 pfr = vec2(-1.), pto = vec2(-1.);
  float middleSlit = 2.0 * abs(p.x-0.5) - progress;
  if (middleSlit > 0.0) {
    pfr = p + (p.x > 0.5 ? -1.0 : 1.0) * vec2(0.5*progress, 0.0);
    float d = 1.0/(1.0+kParamFloat1*progress*(1.0-middleSlit));
    pfr.y -= d/2.;
    pfr.y *= d;
    pfr.y += d/2.;
  }
  float size = mix(1.0, kParamFloat2, 1.-progress);
  pto = (p + vec2(-0.5, -0.5)) * vec2(size, size) + vec2(0.5, 0.5);
  if (inBounds(pfr)) {
    return getFromColor(pfr);
  }
  else if (inBounds(pto)) {
    return getToColor(pto);
  }
  else {
    return bgColor(p, pto);
  }
}
",
    },
    GlPreset {
        name: "Dots Curtain",
        values: &[
            ("IsPreset", "9"),
            ("kParamFloat0", "20"),
            ("kParamVec20", "0 0"),
            ("kParamFloat0Name", "Dots"),
            ("kParamVec20Name", "Center"),
        ],
        code: "// author: bobylito
// license: MIT
const float SQRT_2 = 1.414213562373;
uniform float kParamFloat0;// = 20.0; [2,40]
uniform vec2 kParamVec20;// = vec2(0, 0); [-1,-1,1,1]

vec4 transition(vec2 uv) {
  bool nextImage = distance(fract(uv * kParamFloat0), vec2(0.5, 0.5)) < ( progress / distance(uv, kParamVec20));
  return nextImage ? getToColor(uv) : getFromColor(uv);
}
",
    },
    GlPreset {
        name: "Dreamy",
        values: &[
            ("IsPreset", "20"),
        ],
        code: "// Author: mikolalysenko
// License: MIT

vec2 offset(float progress, float x, float theta) {
  float phase = progress*progress + progress + theta;
  float shifty = 0.03*progress*cos(10.0*(progress+x));
  return vec2(0, shifty);
}
vec4 transition(vec2 p) {
  return mix(getFromColor(p + offset(progress, p.x, 0.0)), getToColor(p + offset(1.0-progress, p.x, 3.14)), progress);
}
",
    },
    GlPreset {
        name: "Dreamy Zoom",
        values: &[
            ("IsPreset", "32"),
            ("kParamFloat0", "6"),
            ("kParamFloat1", "1.2"),
            ("kParamFloat0Name", "Rotations"),
            ("kParamFloat1Name", "Scale"),
        ],
        code: "// Author: Zeh Fernando
// License: MIT

// Definitions --------
#define DEG2RAD 0.03926990816987241548078304229099 // 1/180*PI


// Transition parameters --------

// In degrees
uniform float kParamFloat0; // = 6;  [-10,10]

// Multiplier
uniform float kParamFloat1; // = 1.2;  [0.1,3]


// The code proper --------

vec4 transition(vec2 uv) {
  // Massage parameters
  float phase = progress < 0.5 ? progress * 2.0 : (progress - 0.5) * 2.0;
  float angleOffset = progress < 0.5 ? mix(0.0, kParamFloat0 * DEG2RAD, phase) : mix(-kParamFloat0 * DEG2RAD, 0.0, phase);
  float newScale = progress < 0.5 ? mix(1.0, kParamFloat1, phase) : mix(kParamFloat1, 1.0, phase);
  
  vec2 center = vec2(0, 0);

  // Calculate the source point
  vec2 assumedCenter = vec2(0.5, 0.5);
  vec2 p = (uv.xy - vec2(0.5, 0.5)) / newScale * vec2(ratio, 1.0);

  // This can probably be optimized (with distance())
  float angle = atan(p.y, p.x) + angleOffset;
  float dist = distance(center, p);
  p.x = cos(angle) * dist / ratio + 0.5;
  p.y = sin(angle) * dist + 0.5;
  vec4 c = progress < 0.5 ? getFromColor(p) : getToColor(p);

  // Finally, apply the color
  return c + (progress < 0.5 ? mix(0.0, 1.0, phase) : mix(1.0, 0.0, phase));
}
",
    },
    GlPreset {
        name: "Flyeye",
        values: &[
            ("IsPreset", "44"),
            ("kParamFloat0", "0.04"),
            ("kParamFloat1", "50"),
            ("kParamFloat2", "0.3"),
            ("kParamFloat0Name", "Size"),
            ("kParamFloat1Name", "Zoom"),
            ("kParamFloat2Name", "Color Separation"),
        ],
        code: "// Author: gre
// License: MIT
uniform float kParamFloat0; // = 0.04;  [0.01,0.5]
uniform float kParamFloat1; // = 50.0; [10,100]
uniform float kParamFloat2; // = 0.3; [0.1,0.6]

vec4 transition(vec2 p) {
  float inv = 1. - progress;
  vec2 disp = kParamFloat0*vec2(cos(kParamFloat1*p.x), sin(kParamFloat1*p.y));
  vec4 texTo = getToColor(p + inv*disp);
  vec4 texFrom = vec4(
    getFromColor(p + progress*disp*(1.0 - kParamFloat2)).r,
    getFromColor(p + progress*disp).g,
    getFromColor(p + progress*disp*(1.0 + kParamFloat2)).b,
    1.0);
  return texTo*progress + texFrom*inv;
}
",
    },
    GlPreset {
        name: "From In",
        values: &[
            ("IsPreset", "54"),
        ],
        code: "// Author: haiyoucuv
// License: MIT

vec4 transition (vec2 uv) {
  
  vec4 coordTo = getToColor(uv);
  vec4 coordFrom = getFromColor(uv);
  
  return mix(
    getFromColor(coordTo.rg),
    getToColor(uv),
    progress
  );

}
",
    },
    GlPreset {
        name: "Glitch Displace",
        values: &[
            ("IsPreset", "31"),
        ],
        code: "// Author: Matt DesLauriers
// License: MIT
// Description: Blocks of the image are displaced in a glitch that flashes into the incoming image
// Tags: glitch, displace

float random(vec2 co)
{
    float a = 12.9898;
    float b = 78.233;
    float c = 43758.5453;
    float dt= dot(co.xy ,vec2(a,b));
    float sn= mod(dt,3.14);
    return fract(sin(sn) * c);
}
float voronoi( in vec2 x ) {
    vec2 p = floor( x );
    vec2 f = fract( x );
    float res = 8.0;
    for( float j=-1.; j<=1.; j++ )
    for( float i=-1.; i<=1.; i++ ) {
        vec2  b = vec2( i, j );
        vec2  r = b - f + random( p + b );
        float d = dot( r, r );
        res = min( res, d );
    }
    return sqrt( res );
}

vec2 displace(vec4 tex, vec2 texCoord, float dotDepth, float textureDepth, float strength) {
    float b = voronoi(.003 * texCoord + 2.0);
    float g = voronoi(0.2 * texCoord);
    float r = voronoi(texCoord - 1.0);
    vec4 dt = tex * 1.0;
    vec4 dis = dt * dotDepth + 1.0 - tex * textureDepth;

    dis.x = dis.x - 1.0 + textureDepth*dotDepth;
    dis.y = dis.y - 1.0 + textureDepth*dotDepth;
    dis.x *= strength;
    dis.y *= strength;
    vec2 res_uv = texCoord ;
    res_uv.x = res_uv.x + dis.x - 0.0;
    res_uv.y = res_uv.y + dis.y;
    return res_uv;
}

float ease1(float t) {
  return t == 0.0 || t == 1.0
    ? t
    : t < 0.5
      ? +0.5 * pow(2.0, (20.0 * t) - 10.0)
      : -0.5 * pow(2.0, 10.0 - (t * 20.0)) + 1.0;
}
float ease2(float t) {
  return t == 1.0 ? t : 1.0 - pow(2.0, -10.0 * t);
}



vec4 transition(vec2 uv) {
  vec2 p = uv.xy / vec2(1.0).xy;
  vec4 color1 = getFromColor(p);
  vec4 color2 = getToColor(p);
  vec2 disp = displace(color1, p, 0.33, 0.7, 1.0-ease1(progress));
  vec2 disp2 = displace(color2, p, 0.33, 0.5, ease2(progress));
  vec4 dColor1 = getToColor(disp);
  vec4 dColor2 = getFromColor(disp2);
  float val = ease1(progress);
  vec3 gray = vec3(dot(min(dColor2, dColor1).rgb, vec3(0.299, 0.587, 0.114)));
  dColor2 = vec4(gray, 1.0);
  dColor2 *= 2.0;
  color1 = mix(color1, dColor2, smoothstep(0.0, 0.5, progress));
  color2 = mix(color2, dColor1, smoothstep(1.0, 0.5, progress));
  return mix(color1, color2, val);
  //gl_FragColor = mix(gl_FragColor, dColor, smoothstep(0.0, 0.5, progress));

   //gl_FragColor = mix(texture2D(from, p), texture2D(to, p), progress);
}
",
    },
    GlPreset {
        name: "Glitch Memories",
        values: &[
            ("IsPreset", "8"),
            ("kParamFloat0", "0.5"),
            ("kParamFloat1", "0.2"),
            ("kParamFloat2", "0.3"),
            ("kParamFloat3", "0.5"),
            ("kParamFloat0Name", "Movement"),
            ("kParamFloat1Name", "Red aberration"),
            ("kParamFloat2Name", "Green aberration"),
            ("kParamFloat3Name", "Blue aberration"),
        ],
        code: "// author: Gunnar Roth
// based on work from natewave
// license: MIT
uniform float kParamFloat0; // = 0.5; [0.0,1.0]
uniform float kParamFloat1; // = 0.2; [0.0,1.0]
uniform float kParamFloat2; // = 0.3; [0.0,1.0]
uniform float kParamFloat3; // = 0.5; [0.0,1.0]
vec4 transition(vec2 p) {
  vec2 block = floor(p.xy / vec2(16));
  vec2 uv_noise = block / vec2(64);
  uv_noise += floor(vec2(progress) * vec2(kParamFloat0 * 1200.0, kParamFloat0 * 3500.0)) / vec2(64);
  vec2 dist = progress > 0.0 ? (fract(uv_noise) - 0.5) * 0.3 *(1.0 -progress) : vec2(0.0);
  vec2 red = p + dist * kParamFloat1;
  vec2 green = p + dist * kParamFloat2;
  vec2 blue = p + dist * kParamFloat3;

  return vec4(mix(getFromColor(red), getToColor(red), progress).r,mix(getFromColor(green), getToColor(green), progress).g,mix(getFromColor(blue), getToColor(blue), progress).b,1.0);
}

",
    },
    GlPreset {
        name: "Gray Fade",
        values: &[
            ("IsPreset", "43"),
            ("kParamFloat0", "0.3"),
            ("kParamFloat0Name", "Intensity"),
        ],
        code: "// Author: gre
// License: MIT

uniform float kParamFloat0; // = 0.3; [0.0,0.9]
 
vec3 grayscale (vec3 color) {
  return vec3(0.2126*color.r + 0.7152*color.g + 0.0722*color.b);
}
 
vec4 transition (vec2 uv) {
  vec4 fc = getFromColor(uv);
  vec4 tc = getToColor(uv);
  return mix(
    mix(vec4(grayscale(fc.rgb), 1.0), fc, smoothstep(1.0-kParamFloat0, 0.0, progress)),
    mix(vec4(grayscale(tc.rgb), 1.0), tc, smoothstep(    kParamFloat0, 1.0, progress)),
    progress);
}
",
    },
    GlPreset {
        name: "Grid Flip",
        values: &[
            ("IsPreset", "21"),
            ("kParamFloat0", "0.1"),
            ("kParamFloat1", "0.05"),
            ("kParamFloat2", "0.1"),
            ("kParamIVec20", "4 4"),
            ("kParamVec40", "0 0 0 1.0"),
            ("kParamFloat0Name", "Pause"),
            ("kParamFloat1Name", "Divider Width"),
            ("kParamFloat2Name", "Randomness"),
            ("kParamIVec20Name", "Grids"),
            ("kParamVec40Name", "Background"),
        ],
        code: "// License: MIT
// Author: TimDonselaar
// ported by gre from https://gist.github.com/TimDonselaar/9bcd1c4b5934ba60087bdb55c2ea92e5

uniform ivec2 kParamIVec20; // = ivec2(4); [1,1,8,8]
uniform float kParamFloat0; // = 0.1; [0,0.3]
uniform float kParamFloat1; // = 0.05; [0,0.2]
uniform vec4 kParamVec40; // = vec4(0.0, 0.0, 0.0, 1.0)
uniform float kParamFloat2; // = 0.1; [0,0.3]
 
float rand (vec2 co) {
  return fract(sin(dot(co.xy ,vec2(12.9898,78.233))) * 43758.5453);
}

float getDelta(vec2 p) {
  vec2 rectanglePos = floor(vec2(kParamIVec20) * p);
  vec2 rectangleSize = vec2(1.0 / vec2(kParamIVec20).x, 1.0 / vec2(kParamIVec20).y);
  float top = rectangleSize.y * (rectanglePos.y + 1.0);
  float bottom = rectangleSize.y * rectanglePos.y;
  float left = rectangleSize.x * rectanglePos.x;
  float right = rectangleSize.x * (rectanglePos.x + 1.0);
  float minX = min(abs(p.x - left), abs(p.x - right));
  float minY = min(abs(p.y - top), abs(p.y - bottom));
  return min(minX, minY);
}

float getDividerSize() {
  vec2 rectangleSize = vec2(1.0 / vec2(kParamIVec20).x, 1.0 / vec2(kParamIVec20).y);
  return min(rectangleSize.x, rectangleSize.y) * kParamFloat1;
}

vec4 transition(vec2 p) {
  if(progress < kParamFloat0) {
    float currentProg = progress / kParamFloat0;
    float a = 1.0;
    if(getDelta(p) < getDividerSize()) {
      a = 1.0 - currentProg;
    }
    return mix(kParamVec40, getFromColor(p), a);
  }
  else if(progress < 1.0 - kParamFloat0){
    if(getDelta(p) < getDividerSize()) {
      return kParamVec40;
    } else {
      float currentProg = (progress - kParamFloat0) / (1.0 - kParamFloat0 * 2.0);
      vec2 q = p;
      vec2 rectanglePos = floor(vec2(kParamIVec20) * q);
      
      float r = rand(rectanglePos) - kParamFloat2;
      float cp = smoothstep(0.0, 1.0 - r, currentProg);
    
      float rectangleSize = 1.0 / vec2(kParamIVec20).x;
      float delta = rectanglePos.x * rectangleSize;
      float offset = rectangleSize / 2.0 + delta;
      
      p.x = (p.x - offset)/abs(cp - 0.5)*0.5 + offset;
      vec4 a = getFromColor(p);
      vec4 b = getToColor(p);
      
      float s = step(abs(vec2(kParamIVec20).x * (q.x - delta) - 0.5), abs(cp - 0.5));
      return mix(kParamVec40, mix(b, a, step(cp, 0.5)), s);
    }
  }
  else {
    float currentProg = (progress - 1.0 + kParamFloat0) / kParamFloat0;
    float a = 1.0;
    if(getDelta(p) < getDividerSize()) {
      a = currentProg;
    }
    return mix(kParamVec40, getToColor(p), a);
  }
}
",
    },
    GlPreset {
        name: "Hexagonalize",
        values: &[
            ("IsPreset", "30"),
            ("kParamInt0", "50"),
            ("kParamFloat0", "20"),
            ("kParamInt0Name", "Steps"),
            ("kParamFloat0Name", "horizontal Hexagons"),
        ],
        code: "// Author: Fernando Kuteken
// License: MIT
// Hexagonal math from: http://www.redblobgames.com/grids/hexagons/

uniform int kParamInt0; // = 50; [10,100]
uniform float kParamFloat0; //= 20; [5,50]

struct Hexagon {
  float q;
  float r;
  float s;
};

Hexagon createHexagon(float q, float r){
  Hexagon hex;
  hex.q = q;
  hex.r = r;
  hex.s = -q - r;
  return hex;
}

Hexagon roundHexagon(Hexagon hex){
  
  float q = floor(hex.q + 0.5);
  float r = floor(hex.r + 0.5);
  float s = floor(hex.s + 0.5);

  float deltaQ = abs(q - hex.q);
  float deltaR = abs(r - hex.r);
  float deltaS = abs(s - hex.s);

  if (deltaQ > deltaR && deltaQ > deltaS)
    q = -r - s;
  else if (deltaR > deltaS)
    r = -q - s;
  else
    s = -q - r;

  return createHexagon(q, r);
}

Hexagon hexagonFromPoint(vec2 point, float size) {
  
  point.y /= ratio;
  point = (point - 0.5) / size;
  
  float q = (sqrt(3.0) / 3.0) * point.x + (-1.0 / 3.0) * point.y;
  float r = 0.0 * point.x + 2.0 / 3.0 * point.y;

  Hexagon hex = createHexagon(q, r);
  return roundHexagon(hex);
  
}

vec2 pointFromHexagon(Hexagon hex, float size) {
  
  float x = (sqrt(3.0) * hex.q + (sqrt(3.0) / 2.0) * hex.r) * size + 0.5;
  float y = (0.0 * hex.q + (3.0 / 2.0) * hex.r) * size + 0.5;
  
  return vec2(x, y * ratio);
}

vec4 transition (vec2 uv) {
  
  float dist = 2.0 * min(progress, 1.0 - progress);
  dist = kParamInt0 > 0 ? ceil(dist * float(kParamInt0)) / float(kParamInt0) : dist;
  
  float size = (sqrt(3.0) / 3.0) * dist / kParamFloat0;
  
  vec2 point = dist > 0.0 ? pointFromHexagon(hexagonFromPoint(uv, size), size) : uv;

  return mix(getFromColor(point), getToColor(point), progress);
  
}
",
    },
    GlPreset {
        name: "Honeycomb",
        values: &[
            ("IsPreset", "58"),
        ],
        code: "// Author:linjunping
// License: MIT

//蜂窝
#define PI 3.141592654
#define ESP 0.0001

vec2 rotateXAndScaled(vec2 pos, vec2 center, float theta, float scale) {
    pos -= center;
    pos = vec2(dot(vec2(cos(theta), sin(theta)), pos), dot(vec2(-sin(theta), cos(theta)), pos));
    pos *= scale;
    pos += center;
    return pos;
}
float and(float x, float y) {
    if(x > ESP && y > ESP)
        return 1.0;
    else
        return 0.0;
}
float xor(float x, float y) {
    if(abs(x - y) > ESP)
        return 1.0;
    else
        return 0.0;
}
float or(float x, float y) {
    if(x < ESP && y < ESP)
        return 0.;
    else
        return 1.0;
}
float not_(float x) {
    return 1.0 - x;
}
vec2 comb(vec2 texCoord, vec2 grid) {
    vec2 posLocal = fract(texCoord * grid);
    posLocal -= 0.5;
    float lineWidth = 0.1;
    float scaleX = sqrt(3.);
    posLocal.x *= scaleX;
    vec2 posLocal60 = rotateXAndScaled(posLocal, vec2(0), PI / 3., 1.);
    vec2 posLocal120 = rotateXAndScaled(posLocal, vec2(0), 2. * PI / 3., 1.);
    float k = 1. / ((1. - lineWidth) / 2.);
    float v1 = floor(abs(posLocal.y) * k);
    float v2 = floor(abs(posLocal60.y) * k);
    float v3 = floor(abs(posLocal120.y) * k);
    float k1 = 1.0 / (0.5 + lineWidth / 2.0);
    float v4 = floor(abs(posLocal.y) * k1);
    float v5 = floor(abs(posLocal60.y) * k1);
    float v6 = floor(abs(posLocal120.y) * k1);
    float k2 = 1.0 / (lineWidth / 2.0);
    float v7 = floor(abs(posLocal.y) * k2);
    float v8 = xor(or(or(v1, v2), v3), or(or(v4, v5), v6));
    float v9 = or(not_(or(or(v4, v5), v6)), v7);
    float patternColor = and(v9, not_(v8));
    return vec2(patternColor, or(or(v1, v2), v3));
}
float combNum(vec2 pattern, vec2 pos, vec2 grid) {
    vec2 gridNum = floor(pos * grid), gridLoc = floor(fract(pos * grid) - 0.5);
    float num = (gridNum.x) * (grid.y * 2.0 + 1.0);
    num += (1.0 - pattern.y) * (grid.y + 1.0 + gridNum.y + 1.0);
    num += (pattern.y) * ((2.0 * gridLoc.x + 2.0) * grid.y + 1.0 + gridLoc.x + 1.0 + gridLoc.y + gridNum.y);
    return num;
}
float random(float x) {
    return fract(sin(x * 23.371531) * 15812.1231 + cos(x * 65.76) * 89.123);
}
vec4 honeyComb(vec2 texCoord) {
    float ratio=progress;
    float rTime = 0.8;
    float u_ratio = clamp(ratio / rTime, 0.0, 1.0);
    vec2 grid = vec2(10, 10);
    vec2 center = vec2(0.5, 0.5);
    float theta = u_ratio * PI / 2.0;
    vec2 pos1 = rotateXAndScaled(texCoord, center, theta, 1.0 - 0.8 * u_ratio);
    vec2 pos2 = rotateXAndScaled(texCoord, center, -PI / 2.0 + theta, 6.0 - 5.0 * u_ratio);
    vec4 texColor1 = vec4(0.0), texColor2 = vec4(0.0);
    if(pos1.x > 0.0 && pos1.x < 1.0 && pos1.y > 0.0 && pos1.y < 1.0) {
        texColor1 = getFromColor(pos1);
    }
    if(pos2.x > 0.0 && pos2.x < 1.0 && pos2.y > 0.0 && pos2.y < 1.0) {
        texColor2 = getToColor(pos2);
    }
    vec2 pattern1 = comb(pos1, grid);
    vec2 pattern2 = comb(pos2, grid);
    float num1 = combNum(pattern1, pos1, grid);
    float num2 = combNum(pattern2, pos2, grid);
    float r1 = random(num1), r2 = random(num2);
    float alpha1 = floor(u_ratio - r1 + 1.0), alpha2 = floor(u_ratio - r2 + 1.0);
    vec4 colo2 = pattern2.x * alpha2 * texColor2, colo1 = pattern1.x * (1.0 - alpha1) * texColor1;
    vec4 color3 = mix(colo1, colo2, max(alpha1, 1.0 - pattern1.x));
    float k = -4.0, alpha3 = k * (texCoord.x - 1.0) + 1.0 + (ratio - rTime) / (1.0 - rTime) * (-1.0 + k);
    return mix(texColor2, color3, clamp(alpha3, 0.0, 1.0));
}

vec4 transition (vec2 uv) {
  return honeyComb(uv);
}
",
    },
    GlPreset {
        name: "Kaleidoscope",
        values: &[
            ("IsPreset", "28"),
            ("kParamFloat0", "1"),
            ("kParamFloat1", "1"),
            ("kParamFloat2", "1.5"),
            ("kParamFloat0Name", "Speed"),
            ("kParamFloat1Name", "Angle"),
            ("kParamFloat2Name", "Power"),
        ],
        code: "// Author: nwoeanhinnogaehr
// License: MIT

uniform float kParamFloat0; // = 1.0; [0.1,3]
uniform float kParamFloat1; // = 1.0; [0.5,5]
uniform float kParamFloat2; // = 1.5; [0.5,5]

vec4 transition(vec2 uv) {
  vec2 p = uv.xy / vec2(1.0).xy;
  vec2 q = p;
  float t = pow(progress, kParamFloat2)*kParamFloat0;
  p = p -0.5;
  for (int i = 0; i < 7; i++) {
    p = vec2(sin(t)*p.x + cos(t)*p.y, sin(t)*p.y - cos(t)*p.x);
    t += kParamFloat1;
    p = abs(mod(p, 2.0) - 1.0);
  }
  abs(mod(p, 1.0));
  return mix(
    mix(getFromColor(q), getToColor(q), progress),
    mix(getFromColor(p), getToColor(p), progress), 1.0 - 2.0*abs(progress - 0.5));
}
",
    },
    GlPreset {
        name: "Linear Blur",
        values: &[
            ("IsPreset", "6"),
            ("kParamFloat0", "0.1"),
            ("kParamFloat0Name", "Intensity"),
        ],
        code: "// author: gre
// license: MIT
uniform float kParamFloat0; // = 0.1; [0,0.2]
const int passes = 6;

vec4 transition(vec2 uv) {
    vec4 c1 = vec4(0.0);
    vec4 c2 = vec4(0.0);

    float disp = kParamFloat0*(0.5-distance(0.5, progress));
    for (int xi=0; xi<passes; xi++)
    {
        float x = float(xi) / float(passes) - 0.5;
        for (int yi=0; yi<passes; yi++)
        {
            float y = float(yi) / float(passes) - 0.5;
            vec2 v = vec2(x,y);
            float d = disp;
            c1 += getFromColor( uv + d*v);
            c2 += getToColor( uv + d*v);
        }
    }
    c1 /= float(passes*passes);
    c2 /= float(passes*passes);
    return mix(c1, c2, progress);
}
",
    },
    GlPreset {
        name: "Luminance Melt",
        values: &[
            ("IsPreset", "11"),
            ("kParamFloat0", "0.8"),
            ("kParamFloat0Name", "Luminance Threshold"),
        ],
        code: "// Author: 0gust1
// License: MIT
//My own first transition — based on crosshatch code (from pthrasher), using  simplex noise formula (copied and pasted)
//-> cooler with high contrasted images (isolated dark subject on light background f.e.)
//TODO : try to rebase it on DoomTransition (from zeh)?
//optimizations :
//luminance (see http://stackoverflow.com/questions/596216/formula-to-determine-brightness-of-rgb-color#answer-596241)
// Y = (R+R+B+G+G+G)/6
//or Y = (R+R+R+B+G+G+G+G)>>3 


//direction of movement :  0 : up, 1, down
const bool direction = true;
//luminance threshold
uniform float kParamFloat0; // = 0.8; [0.1,1]
//does the movement takes effect above or below luminance threshold ?
const bool above = false;


//Random function borrowed from everywhere
float rand(vec2 co){
  return fract(sin(dot(co.xy ,vec2(12.9898,78.233))) * 43758.5453);
}


// Simplex noise :
// Description : Array and textureless GLSL 2D simplex noise function.
//      Author : Ian McEwan, Ashima Arts.
//  Maintainer : ijm
//     Lastmod : 20110822 (ijm)
//     License : MIT  
//               2011 Ashima Arts. All rights reserved.
//               Distributed under the MIT License. See LICENSE file.
//               https://github.com/ashima/webgl-noise
// 

vec3 mod289(vec3 x) {
  return x - floor(x * (1.0 / 289.0)) * 289.0;
}

vec2 mod289(vec2 x) {
  return x - floor(x * (1.0 / 289.0)) * 289.0;
}

vec3 permute(vec3 x) {
  return mod289(((x*34.0)+1.0)*x);
}

float snoise(vec2 v)
  {
  const vec4 C = vec4(0.211324865405187,  // (3.0-sqrt(3.0))/6.0
                      0.366025403784439,  // 0.5*(sqrt(3.0)-1.0)
                     -0.577350269189626,  // -1.0 + 2.0 * C.x
                      0.024390243902439); // 1.0 / 41.0
// First corner
  vec2 i  = floor(v + dot(v, C.yy) );
  vec2 x0 = v -   i + dot(i, C.xx);

// Other corners
  vec2 i1;
  //i1.x = step( x0.y, x0.x ); // x0.x > x0.y ? 1.0 : 0.0
  //i1.y = 1.0 - i1.x;
  i1 = (x0.x > x0.y) ? vec2(1.0, 0.0) : vec2(0.0, 1.0);
  // x0 = x0 - 0.0 + 0.0 * C.xx ;
  // x1 = x0 - i1 + 1.0 * C.xx ;
  // x2 = x0 - 1.0 + 2.0 * C.xx ;
  vec4 x12 = x0.xyxy + C.xxzz;
  x12.xy -= i1;

// Permutations
  i = mod289(i); // Avoid truncation effects in permutation
  vec3 p = permute( permute( i.y + vec3(0.0, i1.y, 1.0 ))
        + i.x + vec3(0.0, i1.x, 1.0 ));

  vec3 m = max(0.5 - vec3(dot(x0,x0), dot(x12.xy,x12.xy), dot(x12.zw,x12.zw)), 0.0);
  m = m*m ;
  m = m*m ;

// Gradients: 41 points uniformly over a line, mapped onto a diamond.
// The ring size 17*17 = 289 is close to a multiple of 41 (41*7 = 287)

  vec3 x = 2.0 * fract(p * C.www) - 1.0;
  vec3 h = abs(x) - 0.5;
  vec3 ox = floor(x + 0.5);
  vec3 a0 = x - ox;

// Normalise gradients implicitly by scaling m
// Approximation of: m *= inversesqrt( a0*a0 + h*h );
  m *= 1.79284291400159 - 0.85373472095314 * ( a0*a0 + h*h );

// Compute final noise value at P
  vec3 g;
  g.x  = a0.x  * x0.x  + h.x  * x0.y;
  g.yz = a0.yz * x12.xz + h.yz * x12.yw;
  return 130.0 * dot(m, g);
}

// Simplex noise -- end

float luminance(vec4 color){
  //(0.299*R + 0.587*G + 0.114*B)
  return color.r*0.299+color.g*0.587+color.b*0.114;
}

vec2 center = vec2(1.0, direction);

vec4 transition(vec2 uv) {
  vec2 p = uv.xy / vec2(1.0).xy;
  if (progress == 0.0) {
    return getFromColor(p);
  } else if (progress == 1.0) {
    return getToColor(p);
  } else {
    float x = progress;
    float dist = distance(center, p)- progress*exp(snoise(vec2(p.x, 0.0)));
    float r = x - rand(vec2(p.x, 0.1));
    float m;
    if(above){
     m = dist <= r && luminance(getFromColor(p))>kParamFloat0 ? 1.0 : (progress*progress*progress);
    }
    else{
     m = dist <= r && luminance(getFromColor(p))<kParamFloat0 ? 1.0 : (progress*progress*progress);  
    }
    return mix(getFromColor(p), getToColor(p), m);    
  }
}
",
    },
    GlPreset {
        name: "Morph",
        values: &[
            ("IsPreset", "15"),
            ("kParamFloat0", "0.1"),
            ("kParamFloat0Name", "Strength"),
        ],
        code: "// Author: paniq
// License: MIT
uniform float kParamFloat0; // = 0.1; [0,1]

vec4 transition(vec2 p) {
  vec4 ca = getFromColor(p);
  vec4 cb = getToColor(p);
  
  vec2 oa = (((ca.rg+ca.b)*0.5)*2.0-1.0);
  vec2 ob = (((cb.rg+cb.b)*0.5)*2.0-1.0);
  vec2 oc = mix(oa,ob,0.5)*kParamFloat0;
  
  float w0 = progress;
  float w1 = 1.0-w0;
  return mix(getFromColor(p+oc*w0), getToColor(p-oc*w1), progress);
}
",
    },
    GlPreset {
        name: "Mosaic",
        values: &[
            ("IsPreset", "23"),
            ("kParamInt0", "2"),
            ("kParamInt1", "-1"),
            ("kParamInt0Name", "X"),
            ("kParamInt1Name", "Y"),
        ],
        code: "// License: MIT
// Author: Xaychru
// ported by gre from https://gist.github.com/Xaychru/130bb7b7affedbda9df5

#define PI 3.14159265358979323
#define POW2(X) X*X
#define POW3(X) X*X*X
uniform int kParamInt0; // = 2; [-4,4]
uniform int kParamInt1; // = -1;[-4,4]

float Rand(vec2 v) {
  return fract(sin(dot(v.xy ,vec2(12.9898,78.233))) * 43758.5453);
}
vec2 Rotate(vec2 v, float a) {
  mat2 rm = mat2(cos(a), -sin(a),
                 sin(a), cos(a));
  return rm*v;
}
float CosInterpolation(float x) {
  return -cos(x*PI)/2.+.5;
}
vec4 transition(vec2 uv) {
  vec2 p = uv.xy / vec2(1.0).xy - .5;
  vec2 rp = p;
  float rpr = (progress*2.-1.);
  float z = -(rpr*rpr*2.) + 3.;
  float az = abs(z);
  rp *= az;
  rp += mix(vec2(.5, .5), vec2(float(kParamInt0) + .5, float(kParamInt1) + .5), POW2(CosInterpolation(progress)));
  vec2 mrp = mod(rp, 1.);
  vec2 crp = rp;
  bool onEnd = int(floor(crp.x))==kParamInt0&&int(floor(crp.y))==kParamInt1;
  if(!onEnd) {
    float ang = float(int(Rand(floor(crp))*4.))*.5*PI;
    mrp = vec2(.5) + Rotate(mrp-vec2(.5), ang);
  }
  if(onEnd || Rand(floor(crp))>.5) {
    return getToColor(mrp);
  } else {
    return getFromColor(mrp);
  }
}
",
    },
    GlPreset {
        name: "Multiply Blend",
        values: &[
            ("IsPreset", "45"),
        ],
        code: "// Author: Fernando Kuteken
// License: MIT

vec4 blend(vec4 a, vec4 b) {
  return a * b;
}

vec4 transition (vec2 uv) {
  
  vec4 blended = blend(getFromColor(uv), getToColor(uv));
  
  if (progress < 0.5)
    return mix(getFromColor(uv), blended, 2.0 * progress);
  else
    return mix(blended, getToColor(uv), 2.0 * progress - 1.0);
}

",
    },
    GlPreset {
        name: "Over Exposure",
        values: &[
            ("IsPreset", "56"),
            ("kParamFloat0", "0.6"),
            ("kParamFloat0Name", "Strength"),
        ],
        code: "// Author: Ben Zhang
// License: MIT

uniform float kParamFloat0; //= 0.6; [0.3, 2]
const float PI = 3.141592653589793;

vec4 transition (vec2 uv) {
  vec4 from = getFromColor(uv);
  vec4 to = getToColor(uv);

  // Multipliers
  float from_m = 1.0 - progress + sin(PI * progress) * kParamFloat0;
  float to_m = progress + sin(PI * progress) * kParamFloat0;
  
  return vec4(
    from.r * from.a * from_m + to.r * to.a * to_m,
    from.g * from.a * from_m + to.g * to.a * to_m,
    from.b * from.a * from_m + to.b * to.a * to_m,
    mix(from.a, to.a, progress)
  );
}
",
    },
    GlPreset {
        name: "Perlin",
        values: &[
            ("IsPreset", "12"),
            ("kParamFloat0", "4"),
            ("kParamFloat1", "0.01"),
            ("kParamFloat2", "12.9898"),
            ("kParamFloat0Name", "Scale"),
            ("kParamFloat1Name", "Smoothness"),
            ("kParamFloat2Name", "Seed"),
        ],
        code: "// Author: Rich Harris
// License: MIT
// Description: The incoming image appears through blobs of Perlin noise
// Tags: noise, dissolve

// @param Scale of the noise; higher values make smaller blobs
uniform float kParamFloat0; // = 4.0; [1,8]
// @param Softness of the blob edges
uniform float kParamFloat1; // = 0.01; [0.01,0.2]

// @param Random kParamFloat2
uniform float kParamFloat2; // = 12.9898; [5,100]

// http://byteblacksmith.com/improvements-to-the-canonical-one-liner-glsl-rand-for-opengl-es-2-0/
float random(vec2 co)
{
    float a = kParamFloat2;
    float b = 78.233;
    float c = 43758.5453;
    float dt= dot(co.xy ,vec2(a,b));
    float sn= mod(dt,3.14);
    return fract(sin(sn) * c);
}

// 2D Noise based on Morgan McGuire @morgan3d
// https://www.shadertoy.com/view/4dS3Wd
float noise (in vec2 st) {
    vec2 i = floor(st);
    vec2 f = fract(st);

    // Four corners in 2D of a tile
    float a = random(i);
    float b = random(i + vec2(1.0, 0.0));
    float c = random(i + vec2(0.0, 1.0));
    float d = random(i + vec2(1.0, 1.0));

    // Smooth Interpolation

    // Cubic Hermine Curve.  Same as SmoothStep()
    vec2 u = f*f*(3.0-2.0*f);
    // u = smoothstep(0.,1.,f);

    // Mix 4 coorners porcentages
    return mix(a, b, u.x) +
            (c - a)* u.y * (1.0 - u.x) +
            (d - b) * u.x * u.y;
}

vec4 transition (vec2 uv) {
  vec4 from = getFromColor(uv);
  vec4 to = getToColor(uv);
  float n = noise(uv * kParamFloat0);

  float p = mix(-kParamFloat1, 1.0 + kParamFloat1, progress);
  float lower = p - kParamFloat1;
  float higher = p + kParamFloat1;

  float q = smoothstep(lower, higher, n);

  return mix(
    from,
    to,
    1.0 - q
  );
}
",
    },
    GlPreset {
        name: "Pin Wheel",
        values: &[
            ("IsPreset", "35"),
            ("kParamFloat0", "2"),
            ("kParamFloat0Name", "Speed"),
        ],
        code: "// Author: Mr Speaker
// License: MIT

uniform float kParamFloat0; // = 2.0; [0.1,4]

vec4 transition(vec2 uv) {
  
  vec2 p = uv.xy / vec2(1.0).xy;
  
  float circPos = atan(p.y - 0.5, p.x - 0.5) + progress * kParamFloat0;
  float modPos = mod(circPos, 3.1415 / 4.);
  float signed = sign(progress - modPos);
  
  return mix(getToColor(p), getFromColor(p), step(signed, 0.5));
  
}
",
    },
    GlPreset {
        name: "Pixelize",
        values: &[
            ("IsPreset", "46"),
            ("kParamInt0", "50"),
            ("kParamIVec20", "20 20"),
            ("kParamIVec20Name", "Squares"),
            ("kParamInt0Name", "Steps"),
        ],
        code: "// Author: gre
// License: MIT
// forked from https://gist.github.com/benraziel/c528607361d90a072e98

// minimum number of squares (when the effect is at its higher level)
uniform ivec2 kParamIVec20; // = ivec2(20,20); [3,3,40,40] // minimum number of squares (when the effect is at its higher level)
// zero disable the stepping
uniform int kParamInt0; // = 50; [5,60] // zero disable the stepping

float d = min(progress, 1.0 - progress);
float dist = kParamInt0>0 ? ceil(d * float(kParamInt0)) / float(kParamInt0) : d;
vec2 squareSize = 2.0 * dist / vec2(kParamIVec20);

vec4 transition(vec2 uv) {
  vec2 p = dist>0.0 ? (floor(uv / squareSize) + 0.5) * squareSize : uv;
  return mix(getFromColor(p), getToColor(p), progress);
}
",
    },
    GlPreset {
        name: "Power Kaleido",
        values: &[
            ("IsPreset", "57"),
            ("kParamFloat0", "2"),
            ("kParamFloat1", "0.5"),
            ("kParamFloat2", "5"),
            ("kParamFloat0Name", "Scale"),
            ("kParamFloat1Name", "Depth"),
            ("kParamFloat2Name", "Speed"),
        ],
        code: "// Name: Power Kaleido
// Author: Boundless
// License: MIT
#define PI 3.14159265358979
const float rad = 120.; // change this value to get different mirror effects
const float deg = rad / 180. * PI;
uniform float kParamFloat0; // = 2.0; [1,5]
uniform float kParamFloat1; // = 1.5; [0.5,5]
float dist = kParamFloat0 / 10.;
uniform float kParamFloat2; // = 5.; [-10,10]
vec2 refl(vec2 p,vec2 o,vec2 n)
{
    return 2.0*o+2.0*n*dot(p-o,n)-p;
}

vec2 rot(vec2 p, vec2 o, float a)
{
    float s = sin(a);
    float c = cos(a);
    return o + mat2(c, -s, s, c) * (p - o);
}

vec4 mainImage(vec2 uv)
{
  vec2 uv0 = uv;
    uv -= 0.5;
  uv.x *= ratio;
  uv *= kParamFloat1;
  uv = rot(uv, vec2(0.0), progress*kParamFloat2);
  // uv.x = fract(uv.x/l/3.0)*l*3.0;
    float theta = progress*6.+PI/.5;
    for(int iter = 0; iter < 10; iter++) {
    for(float i = 0.; i < 2. * PI; i+=deg) {
        float ts = sign(asin(cos(i))) == 1.0 ? 1.0 : 0.0;
      if(((ts == 1.0) && (uv.y-dist*cos(i) > tan(i)*(uv.x+dist*+sin(i)))) || ((ts == 0.0) && (uv.y-dist*cos(i) < tan(i)*(uv.x+dist*+sin(i))))) {
        uv = refl(vec2(uv.x+sin(i)*dist*2.,uv.y-cos(i)*dist*2.), vec2(0.,0.), vec2(cos(i),sin(i)));
      }
    }
  }
  uv += 0.5;
  uv = rot(uv, vec2(0.5), progress*-kParamFloat2);
  uv -= 0.5;
  uv.x /= ratio;
  uv += 0.5;
  uv = 2.*abs(uv/2.-floor(uv/2.+0.5));
  vec2 uvMix = mix(uv,uv0,cos(progress*PI*2.)/2.+0.5);
  vec4 color = mix(getFromColor(uvMix),getToColor(uvMix),cos((progress-1.)*PI)/2.+0.5);
    return color;
    
}
vec4 transition (vec2 uv) {
  vec4 color = mainImage(uv);
  return color;
}
",
    },
    GlPreset {
        name: "Random Squares",
        values: &[
            ("IsPreset", "47"),
            ("kParamFloat0", "0.5"),
            ("kParamIVec20", "10 10"),
            ("kParamIVec20Name", "Size"),
            ("kParamFloat0Name", "Smoothness"),
        ],
        code: "// Author: gre
// License: MIT

uniform ivec2 kParamIVec20; // = ivec2(10, 10); [2,2,30,30]
uniform float kParamFloat0; // = 0.5; [0.1,0.9]
 
float rand (vec2 co) {
  return fract(sin(dot(co.xy ,vec2(12.9898,78.233))) * 43758.5453);
}

vec4 transition(vec2 p) {
  float r = rand(floor(vec2(kParamIVec20) * p));
  float m = smoothstep(0.0, -kParamFloat0, r - (progress * (1.0 + kParamFloat0)));
  return mix(getFromColor(p), getToColor(p), m);
}
",
    },
    GlPreset {
        name: "Ripple",
        values: &[
            ("IsPreset", "34"),
            ("kParamFloat0", "100"),
            ("kParamFloat1", "50"),
            ("kParamFloat0Name", "Amplitude"),
            ("kParamFloat1Name", "Speed"),
        ],
        code: "// Author: gre
// License: MIT
uniform float kParamFloat0; // = 100.0; [1,200]
uniform float kParamFloat1; // = 50.0; [1,100]

vec4 transition (vec2 uv) {
  vec2 dir = uv - vec2(.5);
  float dist = length(dir);
  vec2 offset = dir * (sin(progress * dist * kParamFloat0 - progress * kParamFloat1) + .5) / 30.;
  return mix(
    getFromColor(uv + offset),
    getToColor(uv),
    smoothstep(0.2, 1.0, progress)
  );
}
",
    },
    GlPreset {
        name: "Rotate Scale Fade",
        values: &[
            ("IsPreset", "48"),
            ("kParamVec20", "0.5 0.5"),
            ("kParamFloat0", "1"),
            ("kParamFloat1", "8"),
            ("kParamVec40", "0.15 0.15 0.15 1.0"),
            ("kParamVec20Name", "Center"),
            ("kParamFloat0Name", "Rotation"),
            ("kParamFloat1Name", "Scale"),
            ("kParamVec40Name", "Background Color"),
        ],
        code: "// Author: Fernando Kuteken
// License: MIT

#define PI 3.14159265359

uniform vec2 kParamVec20; // = vec2(0.5, 0.5); [0,0,1,1]
uniform float kParamFloat0; // = 1; [-2,2]
uniform float kParamFloat1; // = 8; [2,16]
uniform vec4 kParamVec40; // = vec4(0.15, 0.15, 0.15, 1.0);

vec4 transition (vec2 uv) {
  
  vec2 difference = uv - kParamVec20;
  vec2 dir = normalize(difference);
  float dist = length(difference);
  
  float angle = 2.0 * PI * kParamFloat0 * progress;
  
  float c = cos(angle);
  float s = sin(angle);
  
  float currentScale = mix(kParamFloat1, 1.0, 2.0 * abs(progress - 0.5));
  
  vec2 rotatedDir = vec2(dir.x  * c - dir.y * s, dir.x * s + dir.y * c);
  vec2 rotatedUv = kParamVec20 + rotatedDir * dist / currentScale;
  
  if (rotatedUv.x < 0.0 || rotatedUv.x > 1.0 ||
      rotatedUv.y < 0.0 || rotatedUv.y > 1.0)
    return kParamVec40;
    
  return mix(getFromColor(rotatedUv), getToColor(rotatedUv), progress);
}
",
    },
    GlPreset {
        name: "Squares Wire",
        values: &[
            ("IsPreset", "52"),
            ("kParamIVec20", "10 10"),
            ("kParamVec20", "1 -0.5"),
            ("kParamFloat0", "1.6"),
            ("kParamIVec20Name", "Squares"),
            ("kParamVec20Name", "Direction"),
            ("kParamFloat0Name", "Smoothness"),
        ],
        code: "// Author: gre
// License: MIT
 
uniform ivec2 kParamIVec20;// = ivec2(10,10) [3,3,30,30]
uniform vec2 kParamVec20;// = vec2(1.0, -0.5) [-1,-1,1,1]
uniform float kParamFloat0; // = 1.6; [1.0, 3.0]

const vec2 center = vec2(0.5, 0.5);
vec4 transition (vec2 p) {
  vec2 v = normalize(kParamVec20);
  v /= abs(v.x)+abs(v.y);
  float d = v.x * center.x + v.y * center.y;
  float offset = kParamFloat0;
  float pr = smoothstep(-offset, 0.0, v.x * p.x + v.y * p.y - (d-0.5+progress*(1.+offset)));
  vec2 squarep = fract(p*vec2(kParamIVec20));
  vec2 squaremin = vec2(pr/2.0);
  vec2 squaremax = vec2(1.0 - pr/2.0);
  float a = (1.0 - step(progress, 0.0)) * step(squaremin.x, squarep.x) * step(squaremin.y, squarep.y) * step(squarep.x, squaremax.x) * step(squarep.y, squaremax.y);
  return mix(getFromColor(p), getToColor(p), a);
}
",
    },
    GlPreset {
        name: "Static Wipe",
        values: &[
            ("IsPreset", "61"),
            ("kParamFloat0", "0.5"),
            ("kParamBool0", "true"),
            ("kParamFloat0Name", "Strength"),
            ("kParamBool0Name", "Downward"),
        ],
        code: "// Author: Ben Lucas
// License: MIT

#define PI 3.14159265359

float rnd (vec2 st) {
    return fract(sin(dot(st.xy,
                         vec2(10,70)))*
        12345.5453123);
}

uniform bool kParamBool0; // = true;
uniform float kParamFloat0;// = 0.5; [0.01, 1]

vec4 transition (vec2 uv) {
  

  float span = kParamFloat0*pow(sin(PI*progress),0.5);
  
  float transitionEdge = kParamBool0 ? 1.0-uv.y : uv.y;
  float mixRatio = 1.0 - step(progress, transitionEdge);

  vec4 transitionMix = mix(
    getFromColor(uv),
    getToColor(uv),
    mixRatio
  );
  
  float noiseEnvelope = smoothstep(progress-span, progress, transitionEdge) * (1.0 - smoothstep(progress, progress + span, transitionEdge));
  vec4 noise = vec4(vec3(rnd(uv*(1.0+progress))), 1.0);
  

  return mix(transitionMix, noise, noiseEnvelope);
}
",
    },
    GlPreset {
        name: "Squeeze",
        values: &[
            ("IsPreset", "49"),
            ("kParamFloat0", "0.04"),
            ("kParamFloat0Name", "Color Separation"),
        ],
        code: "// Author: gre
// License: MIT
 
uniform float kParamFloat0; // = 0.04; [0,0.4]
 
vec4 transition (vec2 uv) {
  float y = 0.5 + (uv.y-0.5) / (1.0-progress);
  if (y < 0.0 || y > 1.0) {
     return getToColor(uv);
  }
  else {
    vec2 fp = vec2(uv.x, y);
    vec2 off = progress * vec2(0.0, kParamFloat0);
    vec4 c = getFromColor(fp);
    vec4 cn = getFromColor(fp - off);
    vec4 cp = getFromColor(fp + off);
    return vec4(cn.r, c.g, cp.b, c.a);
  }
}
",
    },
    GlPreset {
        name: "Stereo Viewer",
        values: &[
            ("IsPreset", "10"),
            ("kParamFloat0", "0.88"),
            ("kParamFloat1", "0.22"),
            ("kParamFloat0Name", "Zoom"),
            ("kParamFloat1Name", "Corner Radius"),
        ],
        code: "// Tunable parameters
// How much to kParamFloat0 (out) for the effect ~ 0.5 - 1.0
uniform float kParamFloat0; // = 0.88; [0.5,1]
// Corner radius as a fraction of the image height
uniform float kParamFloat1;  // = 0.22; [0.1,0.3]

// author: Ted Schundler
// license: BSD 2 Clause
// Free for use and modification by anyone with credit

// Copyright (c) 2016, Theodore K Schundler
// All rights reserved.

// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS \"AS IS\" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

///////////////////////////////////////////////////////////////////////////////
// Stereo Viewer Toy Transition                                              //
//                                                                           //
// Inspired by ViewMaster / Image3D image viewer devices.                    //
// This effect is similar to what you see when you press the device's lever. //
// There is a quick kParamFloat0 in / out to make the transition 'valid' for GLSL.io //
///////////////////////////////////////////////////////////////////////////////

const vec4 black = vec4(0.0, 0.0, 0.0, 1.0);
const vec2 c00 = vec2(0.0, 0.0); // the four corner points
const vec2 c01 = vec2(0.0, 1.0);
const vec2 c11 = vec2(1.0, 1.0);
const vec2 c10 = vec2(1.0, 0.0);

// Check if a point is within a given corner
bool in_corner(vec2 p, vec2 corner, vec2 radius) {
  // determine the direction we want to be filled
  vec2 axis = (c11 - corner) - corner;

  // warp the point so we are always testing the bottom left point with the
  // circle centered on the origin
  p = p - (corner + axis * radius);
  p *= axis / radius;
  return (p.x > 0.0 && p.y > -1.0) || (p.y > 0.0 && p.x > -1.0) || dot(p, p) < 1.0;
}

// Check all four corners
// return a float for v2 for anti-aliasing?
bool test_rounded_mask(vec2 p, vec2 corner_size) {
  return
      in_corner(p, c00, corner_size) &&
      in_corner(p, c01, corner_size) &&
      in_corner(p, c10, corner_size) &&
      in_corner(p, c11, corner_size);
}

// Screen blend mode - https://en.wikipedia.org/wiki/Blend_modes
// This more closely approximates what you see than linear blending
vec4 screen(vec4 a, vec4 b) {
  return 1.0 - (1.0 - a) * (1.0 -b);
}

// Given RGBA, find a value that when screened with itself
// will yield the original value.
vec4 unscreen(vec4 c) {
  return 1.0 - sqrt(1.0 - c);
}

// Grab a pixel, only if it isn't masked out by the rounded corners
vec4 sample_with_corners_from(vec2 p, vec2 corner_size) {
  p = (p - 0.5) / kParamFloat0 + 0.5;
  if (!test_rounded_mask(p, corner_size)) {
    return black;
  }
  return unscreen(getFromColor(p));
}

vec4 sample_with_corners_to(vec2 p, vec2 corner_size) {
  p = (p - 0.5) / kParamFloat0 + 0.5;
  if (!test_rounded_mask(p, corner_size)) {
    return black;
  }
  return unscreen(getToColor(p));
}

// special sampling used when zooming - extra kParamFloat0 parameter and don't unscreen
vec4 simple_sample_with_corners_from(vec2 p, vec2 corner_size, float zoom_amt) {
  p = (p - 0.5) / (1.0 - zoom_amt + kParamFloat0 * zoom_amt) + 0.5;
  if (!test_rounded_mask(p, corner_size)) {
    return black;
  }
  return getFromColor(p);
}

vec4 simple_sample_with_corners_to(vec2 p, vec2 corner_size, float zoom_amt) {
  p = (p - 0.5) / (1.0 - zoom_amt + kParamFloat0 * zoom_amt) + 0.5;
  if (!test_rounded_mask(p, corner_size)) {
    return black;
  }
  return getToColor(p);
}

// Basic 2D affine transform matrix helpers
// These really shouldn't be used in a fragment shader - I should work out the
// the math for a translate & rotate function as a pair of dot products instead

mat3 rotate2d(float angle, float ratio) {
  float s = sin(angle);
  float c = cos(angle);
  return mat3(
    c, s ,0.0,
    -s, c, 0.0,
    0.0, 0.0, 1.0);
}

mat3 translate2d(float x, float y) {
  return mat3(
    1.0, 0.0, 0,
    0.0, 1.0, 0,
    -x, -y, 1.0);
}

mat3 scale2d(float x, float y) {
  return mat3(
    x, 0.0, 0,
    0.0, y, 0,
    0, 0, 1.0);
}

// Split an image and rotate one up and one down along off screen pivot points
vec4 get_cross_rotated(vec3 p3, float angle, vec2 corner_size, float ratio) {
  angle = angle * angle; // easing
  angle /= 2.4; // works out to be a good number of radians

  mat3 center_and_scale = translate2d(-0.5, -0.5) * scale2d(1.0, ratio);
  mat3 unscale_and_uncenter = scale2d(1.0, 1.0/ratio) * translate2d(0.5,0.5);
  mat3 slide_left = translate2d(-2.0,0.0);
  mat3 slide_right = translate2d(2.0,0.0);
  mat3 rotate = rotate2d(angle, ratio);

  mat3 op_a = center_and_scale * slide_right * rotate * slide_left * unscale_and_uncenter;
  mat3 op_b = center_and_scale * slide_left * rotate * slide_right * unscale_and_uncenter;

  vec4 a = sample_with_corners_from((op_a * p3).xy, corner_size);
  vec4 b = sample_with_corners_from((op_b * p3).xy, corner_size);

  return screen(a, b);
}

// Image stays put, but this time move two masks
vec4 get_cross_masked(vec3 p3, float angle, vec2 corner_size, float ratio) {
  angle = 1.0 - angle;
  angle = angle * angle; // easing
  angle /= 2.4;

  vec4 img;

  mat3 center_and_scale = translate2d(-0.5, -0.5) * scale2d(1.0, ratio);
  mat3 unscale_and_uncenter = scale2d(1.0 / kParamFloat0, 1.0 / (kParamFloat0 * ratio)) * translate2d(0.5,0.5);
  mat3 slide_left = translate2d(-2.0,0.0);
  mat3 slide_right = translate2d(2.0,0.0);
  mat3 rotate = rotate2d(angle, ratio);

  mat3 op_a = center_and_scale * slide_right * rotate * slide_left * unscale_and_uncenter;
  mat3 op_b = center_and_scale * slide_left * rotate * slide_right * unscale_and_uncenter;

  bool mask_a = test_rounded_mask((op_a * p3).xy, corner_size);
  bool mask_b = test_rounded_mask((op_b * p3).xy, corner_size);

  if (mask_a || mask_b) {
    img = sample_with_corners_to(p3.xy, corner_size);
    return screen(mask_a ? img : black, mask_b ? img : black);
  } else {
    return black;
  }
}

vec4 transition(vec2 uv) {
  float a;
  vec2 p=uv.xy/vec2(1.0).xy;
  vec3 p3 = vec3(p.xy, 1.0); // for 2D matrix transforms

  // corner is warped to represent to size after mapping to 1.0, 1.0
  vec2 corner_size = vec2(kParamFloat1 / ratio, kParamFloat1);

  if (progress <= 0.0) {
    // 0.0: start with the base frame always
    return getFromColor(p);
  } else if (progress < 0.1) {
    // 0.0-0.1: kParamFloat0 out and add rounded corners
    a = progress / 0.1;
    return  simple_sample_with_corners_from(p, corner_size * a, a);
  } else if (progress < 0.48) {
    // 0.1-0.48: Split original image apart
    a = (progress - 0.1)/0.38;
    return get_cross_rotated(p3, a, corner_size, ratio);
  } else if (progress < 0.9) {
    // 0.48-0.52: black
    // 0.52 - 0.9: unmask new image
    return get_cross_masked(p3, (progress - 0.52)/0.38, corner_size, ratio);
  } else if (progress < 1.0) {
    // kParamFloat0 out and add rounded corners
    a = (1.0 - progress) / 0.1;
    return simple_sample_with_corners_to(p, corner_size * a, a);
  } else {
    // 1.0 end with base frame
    return getToColor(p);
  }
}
",
    },
    GlPreset {
        name: "Stretch",
        values: &[
            ("IsPreset", "57"),
            ("kParamInt0", "-1"),
            ("kParamInt0Name", "Direction"),
        ],
        code: "// Author: DanMan
// License: MIT
float left = -2.0;
float top = 0.0;
float intensity = 20.0;
float pi = 3.141592653;
uniform int kParamInt0; // = -1; [-1,1]

vec2 mirror(vec2 v) {
  vec2 m = mod(v, 2.0);
  return mix(m, 2.0 - m, step(1.0, m));
}

float map(float a, float b, float c, float d, float v) {
  return clamp((v - a) * (d - c) / (b - a) + c, 0., 1.);
}

vec4 transition(vec2 uv) {
  vec2 nUv;
  vec2 uvc = uv;
  vec2 vw = uv;
  vec2 vwo = vw;
  float m = progress;
  float steps = 1.0 / (abs(left + top) + 2.);
  float ms = map(steps, 1.0 - steps, 0., 1., m);
  float flip = (m - 0.5) * 2.;
  float signFlip = -sign(left + top);
  float mult = sin(m * pi);

  if (kParamInt0 == 1) {
      vw.x = uv.x * 1. / intensity;
      vw = mix(vwo, vw, mult);
      uv.x = vw.x + (-flip * signFlip > 0. ? .0 * mult : 1. * mult);
      nUv = vec2(uv.x + ms * left, uv.y + ms * top);
  } else {
      vw.x = uv.x * 1. / intensity;
      vw = mix(vwo, vw, mult);
      uv.x = vw.x + (flip * signFlip > 0. ? .0 * mult : 1. * mult);
      nUv = vec2(uv.x - ms * left, uv.y - ms * top);
  }

  nUv = mirror(nUv);
  vec2 nUvO = nUv;
  if(mod(left, 2.0) != 0.) {
      nUv.x *= -1.;
  }

  if(mod(top, 2.0) != 0.) {
      nUv.y *= -1.;
  }

  return mix(getFromColor(nUv), getToColor(nUvO), ms);
}
",
    },
    GlPreset {
        name: "Swap",
        values: &[
            ("IsPreset", "50"),
            ("kParamFloat0", "0.4"),
            ("kParamFloat1", "0.2"),
            ("kParamFloat2", "3"),
            ("kParamFloat0Name", "Reflection"),
            ("kParamFloat1Name", "Perspective"),
            ("kParamFloat2Name", "Depth"),
        ],
        code: "// Author: gre
// License: MIT
// General parameters
uniform float kParamFloat0; // = 0.4; [0,0.5]
uniform float kParamFloat1; // = 0.2; [0.1,0.6]
uniform float kParamFloat2; // = 3.0; [1,5]
 
const vec4 black = vec4(0.0, 0.0, 0.0, 1.0);
const vec2 boundMin = vec2(0.0, 0.0);
const vec2 boundMax = vec2(1.0, 1.0);
 
bool inBounds (vec2 p) {
  return all(lessThan(boundMin, p)) && all(lessThan(p, boundMax));
}
 
vec2 project (vec2 p) {
  return p * vec2(1.0, -1.2) + vec2(0.0, -0.02);
}
 
vec4 bgColor (vec2 p, vec2 pfr, vec2 pto) {
  vec4 c = black;
  pfr = project(pfr);
  if (inBounds(pfr)) {
    c += mix(black, getFromColor(pfr), kParamFloat0 * mix(1.0, 0.0, pfr.y));
  }
  pto = project(pto);
  if (inBounds(pto)) {
    c += mix(black, getToColor(pto), kParamFloat0 * mix(1.0, 0.0, pto.y));
  }
  return c;
}
 
vec4 transition(vec2 p) {
  vec2 pfr, pto = vec2(-1.);
 
  float size = mix(1.0, kParamFloat2, progress);
  float persp = kParamFloat1 * progress;
  pfr = (p + vec2(-0.0, -0.5)) * vec2(size/(1.0-kParamFloat1*progress), size/(1.0-size*persp*p.x)) + vec2(0.0, 0.5);
 
  size = mix(1.0, kParamFloat2, 1.-progress);
  persp = kParamFloat1 * (1.-progress);
  pto = (p + vec2(-1.0, -0.5)) * vec2(size/(1.0-kParamFloat1*(1.0-progress)), size/(1.0-size*persp*(0.5-p.x))) + vec2(1.0, 0.5);

  if (progress < 0.5) {
    if (inBounds(pfr)) {
      return getFromColor(pfr);
    }
    if (inBounds(pto)) {
      return getToColor(pto);
    }  
  }
  if (inBounds(pto)) {
    return getToColor(pto);
  }
  if (inBounds(pfr)) {
    return getFromColor(pfr);
  }
  return bgColor(p, pfr, pto);
}
",
    },
    GlPreset {
        name: "Swirl",
        values: &[
            ("IsPreset", "18"),
            ("kParamFloat0", "1"),
            ("kParamFloat0Name", "Radius"),
        ],
        code: "uniform float kParamFloat0; // = 1.0; [0.8,3]
// License: MIT
// Author: Sergey Kosarevsky
// ( http://www.linderdaum.com )
// ported by gre from https://gist.github.com/corporateshark/cacfedb8cca0f5ce3f7c

vec4 transition(vec2 UV)
{

    float T = progress;

    UV -= vec2( 0.5, 0.5 );

    float Dist = length(UV);

    if ( Dist < kParamFloat0 )
    {
        float Percent = (kParamFloat0 - Dist) / kParamFloat0;
        float A = ( T <= 0.5 ) ? mix( 0.0, 1.0, T/0.5 ) : mix( 1.0, 0.0, (T-0.5)/0.5 );
        float Theta = Percent * Percent * A * 8.0 * 3.14159;
        float S = sin( Theta );
        float C = cos( Theta );
        UV = vec2( dot(UV, vec2(C, -S)), dot(UV, vec2(S, C)) );
    }
    UV += vec2( 0.5, 0.5 );

    vec4 C0 = getFromColor(UV);
    vec4 C1 = getToColor(UV);

    return mix( C0, C1, T );
}
",
    },
    GlPreset {
        name: "Undulating Burn Out",
        values: &[
            ("IsPreset", "24"),
            ("kParamFloat0", "0.03"),
            ("kParamVec20", "0.5 0.5"),
            ("kParamVec40", "0 0 0 0"),
            ("kParamFloat0Name", "Smoothness"),
            ("kParamVec20Name", "Center"),
            ("kParamVec40Name", "Color"),
        ],
        code: "// License: MIT
// Author: pthrasher
// adapted by gre from https://gist.github.com/pthrasher/8e6226b215548ba12734

uniform float kParamFloat0; // = 0.03; [0.01,0.1]
uniform vec2 kParamVec20; // = vec2(0.5); [0,0,1,1]
uniform vec4 kParamVec40; // = vec3(0.0)

const float M_PI = 3.14159265358979323846;

float quadraticInOut(float t) {
  float p = 2.0 * t * t;
  return t < 0.5 ? p : -p + (4.0 * t) - 1.0;
}

float getGradient(float r, float dist) {
  float d = r - dist;
  return mix(
    smoothstep(-kParamFloat0, 0.0, r - dist * (1.0 + kParamFloat0)),
    -1.0 - step(0.005, d),
    step(-0.005, d) * step(d, 0.01)
  );
}

float getWave(vec2 p){
  vec2 _p = p - kParamVec20; // offset from kParamVec20
  float rads = atan(_p.y, _p.x);
  float degs = degrees(rads) + 180.0;
  vec2 range = vec2(0.0, M_PI * 30.0);
  vec2 domain = vec2(0.0, 360.0);
  float ratio = (M_PI * 30.0) / 360.0;
  degs = degs * ratio;
  float x = progress;
  float magnitude = mix(0.02, 0.09, smoothstep(0.0, 1.0, x));
  float offset = mix(40.0, 30.0, smoothstep(0.0, 1.0, x));
  float ease_degs = quadraticInOut(sin(degs));
  float deg_wave_pos = (ease_degs * magnitude) * sin(x * offset);
  return x + deg_wave_pos;
}

vec4 transition(vec2 p) {
  float dist = distance(kParamVec20, p);
  float m = getGradient(getWave(p), dist);
  vec4 cfrom = getFromColor(p);
  vec4 cto = getToColor(p);
  return mix(mix(cfrom, cto, m), mix(cfrom, kParamVec40, 0.75), step(m, -2.0));
}
",
    },
    GlPreset {
        name: "Water Drop",
        values: &[
            ("IsPreset", "7"),
            ("kParamFloat0", "30"),
            ("kParamFloat1", "30"),
            ("kParamFloat0Name", "Amplitude"),
            ("kParamFloat1Name", "Speed"),
        ],
        code: "// author: Paweł Płóciennik
// license: MIT
uniform float kParamFloat0; // = 30; [10,50]
uniform float kParamFloat1; // = 30; [10,50]

vec4 transition(vec2 p) {
  vec2 dir = p - vec2(.5);
  float dist = length(dir);

  if (dist > progress) {
    return mix(getFromColor( p), getToColor( p), progress);
  } else {
    vec2 offset = dir * sin(dist * kParamFloat0 - progress * kParamFloat1);
    return mix(getFromColor( p + offset), getToColor( p), progress);
  }
}
",
    },
    GlPreset {
        name: "Wave",
        values: &[
            ("IsPreset", "53"),
            ("kParamFloat0", "300"),
            ("kParamFloat0Name", "Wavelength"),
        ],
        code: "uniform float kParamFloat0; // = 300.0; [100,1000]
// Author:a
// License: MIT

vec4 transition (vec2 uv) {
  
  vec2 p = uv + 0.01 * vec2(sin(kParamFloat0 * uv.x * progress), cos(kParamFloat0 * uv.y * progress));
  
  return mix(
    getFromColor(p),
    getToColor(uv),
    progress
  );
  
}
",
    },
    GlPreset {
        name: "Wind",
        values: &[
            ("IsPreset", "51"),
            ("kParamFloat0", "0.2"),
            ("kParamFloat0Name", "Size"),
        ],
        code: "// Author: gre
// License: MIT

// Custom parameters
uniform float kParamFloat0; // = 0.2; [0,1]

float rand (vec2 co) {
  return fract(sin(dot(co.xy ,vec2(12.9898,78.233))) * 43758.5453);
}

vec4 transition (vec2 uv) {
  float r = rand(vec2(0, uv.y));
  float m = smoothstep(0.0, -kParamFloat0, uv.x*(1.0-kParamFloat0) + kParamFloat0*r - (progress * (1.0 + kParamFloat0)));
  return mix(
    getFromColor(uv),
    getToColor(uv),
    m
  );
}
",
    },
    GlPreset {
        name: "Window Blinds",
        values: &[
            ("IsPreset", "29"),
        ],
        code: "// Author: Fabien Benetou
// License: MIT

vec4 transition (vec2 uv) {
  float t = progress;
  
  if (mod(floor(uv.y*100.*progress),2.)==0.)
    t*=2.-.5;
  
  return mix(
    getFromColor(uv),
    getToColor(uv),
    mix(t, progress, smoothstep(0.8, 1.0, progress))
  );
}
",
    },
    GlPreset {
        name: "Window Slice",
        values: &[
            ("IsPreset", "2"),
            ("kParamFloat0", "10"),
            ("kParamFloat1", "0.5"),
            ("kParamFloat0Name", "Count"),
            ("kParamFloat1Name", "Smoothness"),
        ],
        code: "// Author: gre
// License: MIT

uniform float kParamFloat0; // = 10.0; [4,30]
uniform float kParamFloat1; // = 0.5; [0.1,1]

vec4 transition (vec2 p) {
  float pr = smoothstep(-kParamFloat1, 0.0, p.x - progress * (1.0 + kParamFloat1));
  float s = step(pr, fract(kParamFloat0 * p.x));
  return mix(getFromColor(p), getToColor(p), s);
}
",
    },
    GlPreset {
        name: "Zoom",
        values: &[
            ("IsPreset", "5"),
            ("kParamFloat0", "0.8"),
            ("kParamFloat0Name", "Zoom Quickness"),
        ],
        code: "// Author: 0gust1
// License: MIT

uniform float kParamFloat0; // = 0.8; [0.2,1]
float nQuick = clamp(kParamFloat0,0.2,1.0);

vec2 zoom(vec2 uv, float amount) {
  return 0.5 + ((uv - 0.5) * (1.0-amount));    
}

vec4 transition (vec2 uv) {
  return mix(
    getFromColor(zoom(uv, smoothstep(0.0, nQuick, progress))),
    getToColor(uv),
   smoothstep(nQuick-0.2, 1.0, progress)
  );
}",
    },
    GlPreset {
        name: "Zoom In Circle",
        values: &[
            ("IsPreset", "22"),
        ],
        code: "// License: MIT
// Author: dycm8009
// ported by gre from https://gist.github.com/dycm8009/948e99b1800e81ad909a

vec2 zoom(vec2 uv, float amount) {
  return 0.5 + ((uv - 0.5) * amount);    
}

vec2 ratio2 = vec2(1.0, 1.0 / ratio);

vec4 transition(vec2 uv) {
  // TODO: some timing are hardcoded but should be one or many parameters
  // TODO: should also be able to configure how much circles
  // TODO: if() branching should be avoided when possible, prefer use of step() & other functions
  vec2 r = 2.0 * ((vec2(uv.xy) - 0.5) * ratio2);
  float pro = progress / 0.8;
  float z = pro * 0.2;
  float t = 0.0;
  if (pro > 1.0) {
    z = 0.2 + (pro - 1.0) * 5.;
    t = clamp((progress - 0.8) / 0.07, 0.0, 1.0);
  }
  if (length(r) < 0.5+z) {
    // uv = zoom(uv, 0.9 - 0.1 * pro);
  }
  else if (length(r) < 0.8+z*1.5) {
    uv = zoom(uv, 1.0 - 0.15 * pro);
    t = t * 0.5;
  }
  else if (length(r) < 1.2+z*2.5) {
    uv = zoom(uv, 1.0 - 0.2 * pro);
    t = t * 0.2;
  }
  else {
    uv = zoom(uv, 1.0 - 0.25 * pro);
  }
  return mix(getFromColor(uv), getToColor(uv), t);
}
",
    },
    GlPreset {
        name: "Zoom Out",
        values: &[
            ("IsPreset", "55"),
            ("kParamFloat0", "2"),
            ("kParamFloat0Name", "Scale"),
        ],
        code: "uniform float kParamFloat0; // = 2; [1.1,2]
// Author:1
// License: MIT

vec4 scale(in vec2 uv, in vec2 scale){
    vec2 s = mix(vec2(1.0), scale, progress) - vec2(1.0);
    uv = 0.5 + (uv - 0.5) * s;
    return getToColor(uv);
}

vec4 transition (vec2 uv) {
  return mix(
    getFromColor(uv),
    scale(uv, vec2(kParamFloat0)),
    progress
  );
}
",
    },
];
