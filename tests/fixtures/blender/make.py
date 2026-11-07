"""Blender's OBJ and MTL exports the tests read and write back: a cube of
five materials (plain, a textured wood with a scaled and moved texture and a
normal map, an emissive one, a transparent glass, a textured metal with coat,
sheen and anisotropy), exported with and without the PBR extension, and its
four textures.

    blender -b --factory-startup --python make.py -- tests/fixtures/blender

Run with a scratch HOME. Made by Blender 5.1.2; the files are this
repository's, under its licence.
"""
import os
import sys

import bpy

out = sys.argv[sys.argv.index('--') + 1]
os.makedirs(out, exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)

def image(name, color, size=4):
    img = bpy.data.images.new(name, size, size)
    img.pixels = list(color) * (size * size)
    img.filepath_raw = os.path.join(out, name + '.png')
    img.file_format = 'PNG'
    img.save()
    return img

def material(name, base=(0.8, 0.8, 0.8, 1), rough=0.5, metal=0.0, tex=None, normal=None, emission=None, alpha=None, scale=None, sheen=None, coat=None, aniso=None, rough_tex=None, metal_tex=None):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    p = nt.nodes['Principled BSDF']
    p.inputs['Base Color'].default_value = base
    p.inputs['Roughness'].default_value = rough
    p.inputs['Metallic'].default_value = metal
    if emission:
        p.inputs['Emission Color'].default_value = emission
        p.inputs['Emission Strength'].default_value = 1.0
    if alpha is not None:
        p.inputs['Alpha'].default_value = alpha
    if sheen is not None:
        p.inputs['Sheen Weight'].default_value = sheen
    if coat is not None:
        p.inputs['Coat Weight'].default_value = coat
        p.inputs['Coat Roughness'].default_value = 0.25
    if aniso is not None:
        p.inputs['Anisotropic'].default_value = aniso
        p.inputs['Anisotropic Rotation'].default_value = 0.125
    def texnode(img, into, noncolor=False):
        t = nt.nodes.new('ShaderNodeTexImage')
        t.image = img
        if noncolor:
            img.colorspace_settings.name = 'Non-Color'
        if scale:
            tc = nt.nodes.new('ShaderNodeTexCoord')
            mp = nt.nodes.new('ShaderNodeMapping')
            mp.inputs['Scale'].default_value = scale[0]
            mp.inputs['Location'].default_value = scale[1]
            nt.links.new(tc.outputs['UV'], mp.inputs['Vector'])
            nt.links.new(mp.outputs['Vector'], t.inputs['Vector'])
        nt.links.new(t.outputs['Color'], into)
        return t
    if tex:
        texnode(tex, p.inputs['Base Color'])
    if rough_tex:
        texnode(rough_tex, p.inputs['Roughness'], True)
    if metal_tex:
        texnode(metal_tex, p.inputs['Metallic'], True)
    if normal:
        t = nt.nodes.new('ShaderNodeTexImage')
        t.image = normal
        normal.colorspace_settings.name = 'Non-Color'
        nm = nt.nodes.new('ShaderNodeNormalMap')
        nm.inputs['Strength'].default_value = 0.5
        nt.links.new(t.outputs['Color'], nm.inputs['Color'])
        nt.links.new(nm.outputs['Normal'], p.inputs['Normal'])
    return m

wood = image('wood', (0.6, 0.4, 0.2, 1))
nrm = image('wood_normal', (0.5, 0.5, 1.0, 1))
rgh = image('wood_roughness', (0.3, 0.3, 0.3, 1))
mtl = image('steel_metallic', (1, 1, 1, 1))

bpy.ops.mesh.primitive_cube_add(size=2)
cube = bpy.context.object
cube.name = 'Cube'
mats = [
    material('Plain'),
    material('Wood', tex=wood, normal=nrm, rough=0.7, scale=((2, 3, 1), (0.25, 0.5, 0))),
    material('Glow', base=(0.1, 0.2, 0.3, 1), emission=(1.0, 0.5, 0.25, 1)),
    material('Glass', base=(0.9, 0.95, 1.0, 1), alpha=0.4, rough=0.05),
    material('Steel', metal=1.0, rough=0.2, rough_tex=rgh, metal_tex=mtl, coat=0.5, sheen=0.3, aniso=0.6),
]
for m in mats:
    cube.data.materials.append(m)
for i, poly in enumerate(cube.data.polygons):
    poly.material_index = i % len(mats)

for pbr in (False, True):
    path = os.path.join(out, 'cube_pbr.obj' if pbr else 'cube.obj')
    bpy.ops.wm.obj_export(filepath=path, export_materials=True, export_pbr_extensions=pbr, path_mode='RELATIVE')
print('done')
