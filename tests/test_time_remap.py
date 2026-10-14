"""Regression tests independent of CUDA, model weights and bundled binaries."""
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, MagicMock

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'src-tauri/resources' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


remap = load('time_remap')
worker = load('rife_worker')


class InputContractTests(unittest.TestCase):
    def test_invalid_factor_is_rejected_before_starting_a_process(self):
        with patch.object(remap.subprocess, 'Popen') as spawn:
            for factor in (0, 1, 2.5, 11, float('nan'), float('inf')):
                with self.subTest(factor=factor), self.assertRaises(ValueError):
                    remap.process_time_remap('source.mp4', factor=factor, output_path='out.mp4')
            spawn.assert_not_called()

    def test_all_slowmo_factors_use_the_requested_pitch_preserving_algorithm(self):
        track = {'start_time': 0, 'duration': 1, 'sample_rate': 48000}
        for factor in range(2, 11):
            result = remap.audio_filter(track, 0, 1, factor, factor)
            self.assertIn(f'rubberband=tempo={1 / factor:.9f}:pitch=1.0', result)
            self.assertNotIn('atempo', result)

    def test_subtitle_slowmo_keeps_text_and_scales_only_timeline_lines(self):
        source = '1\n00:00:01,125 --> 00:00:02,750\nCaption mentions 00:00:01,125\n'
        result = remap.retime_srt(source, 2)
        self.assertIn('00:00:02,250 --> 00:00:05,500', result)
        self.assertIn('Caption mentions 00:00:01,125', result)

    def test_subtitles_use_the_video_origin_and_clip_nonoverlapping_cues(self):
        text = ('1\n00:00:01,000 --> 00:00:01,500\nBefore\n\n'
                '2\n00:00:01,900 --> 00:00:02,250\nOverlap\n\n'
                '3\n00:00:02,300 --> 00:00:02,900\nEnd\n')
        result = remap.normalize_srt(text, 2, 0.5, 2)
        self.assertNotIn('Before', result)
        self.assertIn('00:00:00,000 --> 00:00:00,500\nOverlap', result)
        self.assertIn('00:00:00,600 --> 00:00:01,000\nEnd', result)

    def test_child_environment_cannot_redirect_private_python(self):
        with patch.dict(os.environ, {'PYTHONHOME': 'foreign', 'PYTHONPATH': 'foreign',
                                     'PYTHONIOENCODING': 'cp1252', 'PYTHONUSERBASE': 'foreign'}):
            child = remap.child_environment()
            self.assertNotIn('PYTHONHOME', child)
            self.assertNotIn('PYTHONPATH', child)
            self.assertNotIn('PYTHONUSERBASE', child)
            self.assertEqual(child['PYTHONIOENCODING'], 'utf-8')
            self.assertEqual(os.environ['PYTHONHOME'], 'foreign')

    def test_delayed_audio_is_padded_before_pitch_preserving_slowmo(self):
        track = {'start_time': 2.2, 'duration': 1, 'sample_rate': 48000}
        result = remap.audio_filter(track, 2, 0.5, 1, 2)
        self.assertIn('atrim=start=2.200000000:end=2.500000000', result)
        self.assertIn('adelay=9600S:all=1', result)
        self.assertIn('rubberband=tempo=0.500000000:pitch=1.0', result)
        self.assertNotIn('atempo', result)
        self.assertIn('atrim=duration=1.000000000', result)

    def test_audio_validation_rejects_lost_tracks_timing_and_identity(self):
        original = {'channels': 2, 'channel_layout': 'stereo', 'language': 'eng', 'title': 'Microphone',
                    'disposition': {'default': 1}}
        output = dict(original, start_time=0.0, duration=1.0)
        remap.validate_audio_tracks([original, original], [output, output], 1)
        for tracks in ([output], [dict(output, duration=0.5), output],
                       [dict(output, title='Game'), output], [dict(output, channels=1), output]):
            with self.subTest(tracks=tracks), self.assertRaises(RuntimeError):
                remap.validate_audio_tracks([original, original], tracks, 1)

    def test_mp4_default_audio_selection_is_explicit_without_dropping_tracks(self):
        tracks = [{'disposition': {'default': 0, 'forced': 1}}, {'disposition': {'default': 0}}]
        canonical = remap.canonical_audio_tracks(tracks)
        self.assertEqual([track['disposition']['default'] for track in canonical], [1, 0])
        self.assertEqual(canonical[0]['disposition']['forced'], 1)
        self.assertEqual(tracks[0]['disposition']['default'], 0)

    def test_probe_uses_rational_average_cadence_and_skips_cover_art(self):
        fixture = {'streams': [
            {'codec_type': 'video', 'disposition': {'attached_pic': 1}},
            {'codec_type': 'video', 'width': 720, 'height': 480,
             'r_frame_rate': '60/1', 'avg_frame_rate': '30000/1001', 'duration': '2',
             'sample_aspect_ratio': '8:9', 'nb_frames': '60'},
            {'codec_type': 'audio'},
        ]}
        with patch.object(remap, 'run_command', return_value=json.dumps(fixture)):
            info = remap.get_video_info('source', 'probe')
        self.assertEqual(info['rate'], '30000/1001')
        self.assertEqual(info['sar'], '8/9')
        self.assertEqual(info['width'], 720)
        self.assertEqual(info['video_index'], 1)
        self.assertTrue(info['has_audio'])

    def test_rotation_swaps_dimensions_and_inverts_pixel_aspect(self):
        fixture = {'streams': [{'codec_type': 'video', 'width': 720, 'height': 480,
                   'avg_frame_rate': '24/1', 'duration': '2', 'sample_aspect_ratio': '8:9',
                   'side_data_list': [{'rotation': 90}]}]}
        with patch.object(remap, 'run_command', return_value=json.dumps(fixture)):
            info = remap.get_video_info('source', 'probe')
        self.assertEqual((info['width'], info['height'], info['sar']), (480, 720, '9/8'))

    def test_invalid_video_and_missing_cadence_are_rejected(self):
        for fixture in ({'streams': [{'codec_type': 'audio'}]},
                        {'streams': [{'codec_type': 'video', 'width': 720, 'height': 480,
                                     'avg_frame_rate': '0/0', 'r_frame_rate': '0/0', 'duration': '2'}]}):
            with patch.object(remap, 'run_command', return_value=json.dumps(fixture)):
                with self.assertRaises(ValueError):
                    remap.get_video_info('source', 'probe')

    def test_matroska_video_duration_does_not_use_a_longer_audio_timeline(self):
        fixture = {'streams': [{'index': 0, 'codec_type': 'video', 'width': 96, 'height': 64,
                                'avg_frame_rate': '24/1', 'start_time': '2.0',
                                'tags': {'DURATION': '00:00:02.500000000'}},
                               {'index': 1, 'codec_type': 'audio', 'channels': 1,
                                'tags': {'DURATION': '00:00:03.000000000'}}],
                   'format': {'duration': '3.0'}}
        with patch.object(remap, 'run_command', return_value=json.dumps(fixture)):
            info = remap.get_video_info('source.mkv', 'probe')
        self.assertEqual(info['video_start_time'], 2)
        self.assertEqual(info['duration'], 0.5)
        self.assertEqual(info['audio_tracks'][0]['duration'], 3)

    def test_unknown_video_interval_uses_packets_and_retains_negative_origin(self):
        fixture = {'streams': [{'index': 0, 'codec_type': 'video', 'width': 96, 'height': 64,
                                'avg_frame_rate': '25/1'}], 'format': {'duration': '3.0'}}
        with patch.object(remap, 'run_command', side_effect=[json.dumps(fixture), '-0.040000,0.040000\n0.000000,0.040000\n']):
            info = remap.get_video_info('source.mkv', 'probe')
        self.assertAlmostEqual(info['video_start_time'], -0.04)
        self.assertAlmostEqual(info['duration'], 0.08)


class OwnershipTests(unittest.TestCase):
    def test_preexisting_destination_is_preserved_even_for_bracket_filename(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'source [v1].mp4'
            output = Path(directory) / 'out.mp4'
            source.write_bytes(b'source')
            output.write_bytes(b'existing output')
            with patch.object(remap.subprocess, 'Popen') as spawn:
                with self.assertRaises(FileExistsError):
                    remap.process_time_remap(source, factor=2, output_path=output)
                spawn.assert_not_called()
            self.assertEqual(source.read_bytes(), b'source')
            self.assertEqual(output.read_bytes(), b'existing output')

    def test_encoder_launch_failure_reaps_worker_and_cleans_owned_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            source = directory / 'source.mp4'
            source.write_bytes(b'source')
            model = directory / 'rife/train_log'
            model.mkdir(parents=True)
            for name in ('flownet.pkl', 'RIFE_HDv3.py', 'IFNet_HDv3.py'):
                (model / name).write_bytes(b'fixture')
            process = MagicMock()
            process.stdout = io.BytesIO()
            process.stderr = io.BytesIO()
            process.poll.return_value = None
            process.wait.return_value = 0
            info = {'rate': '24', 'fps': 24, 'width': 16, 'height': 16, 'duration': 1,
                    'transfer': 'bt709', 'sar': '1', 'has_audio': False, 'subtitles': [], 'video_index': 0,
                    'nb_frames': 24, 'nominal_rate': '24', 'video_start_time': 0, 'audio_tracks': []}
            with patch.object(remap, 'get_video_info', return_value=info), \
                 patch.object(remap, 'ensure_tools_in_path'), \
                 patch.object(remap.subprocess, 'Popen', side_effect=[process, OSError('encoder missing')]) as spawn:
                with self.assertRaisesRegex(OSError, 'encoder missing'):
                    remap.process_time_remap(source, factor=2, output_path=directory/'output.mp4',
                                             rife_dir=model.parent)
                self.assertEqual(spawn.call_args_list[0].args[0][1:5], ['-I', '-X', 'utf8', '-u'])
            process.kill.assert_called_once()
            process.wait.assert_called()
            self.assertFalse((directory/'output.mp4').exists())
            self.assertEqual(list(directory.glob('.cia-render-*')), [])
            self.assertEqual(source.read_bytes(), b'source')

    def test_truncated_decoder_frame_is_an_error_not_successful_eof(self):
        self.assertIsNone(worker.read_frame(io.BytesIO(), 6))
        self.assertEqual(worker.read_frame(io.BytesIO(b'123456'), 6), b'123456')
        with self.assertRaisesRegex(RuntimeError, 'incomplete RGB'):
            worker.read_frame(io.BytesIO(b'12345'), 6)


class ModelContractTests(unittest.TestCase):
    def test_plain_and_ddp_checkpoints_load_all_parameters_strictly(self):
        for checkpoint in ({'weight': 'tensor'}, {'module.weight': 'tensor'},
                           {'module.weight': 'tensor', 'module.teacher.weight': 'unused'}):
            with self.subTest(checkpoint=checkpoint):
                model, torch = MagicMock(), MagicMock()
                torch.load.return_value = checkpoint
                worker.load_model_weights(model, torch, 'model')
                model.flownet.load_state_dict.assert_called_once_with({'weight': 'tensor'}, strict=True)
                self.assertTrue(torch.load.call_args.kwargs['weights_only'])

    def test_incomplete_or_duplicate_checkpoint_is_rejected(self):
        model, torch = MagicMock(), MagicMock()
        model.flownet.load_state_dict.side_effect = RuntimeError('Missing key weight')
        torch.load.return_value = {'other': 'tensor'}
        with self.assertRaisesRegex(RuntimeError, 'Missing key'):
            worker.load_model_weights(model, torch, 'model')
        for checkpoint in ({}, {'weight': 'tensor', 'module.weight': 'tensor'}):
            torch.load.return_value = checkpoint
            with self.assertRaises(RuntimeError):
                worker.load_model_weights(model, torch, 'model')


if __name__ == '__main__':
    unittest.main()
