#!/usr/bin/env python3
"""Exercise the deployed Budget Monitor through its real inter-Lyquid read."""
import json,os,subprocess,sys,time,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
ENV=os.environ.copy();ENV['PATH']=str(ROOT/'.tools/bin')+os.pathsep+ENV.get('PATH','')

def main():
    monitor=json.loads((ROOT/'.runtime/budget-monitor.json').read_text())['contract']
    dep=json.loads((ROOT/'.runtime/deployment.json').read_text())
    with urllib.request.urlopen(os.environ.get('TL_BASE_URL','http://127.0.0.1:8080').rstrip('/')+'/api/results',timeout=5) as r:
        results=json.load(r)['results']
    seed=json.loads((ROOT/'.runtime/seed.json').read_text())
    record=next(r for r in results if r['query']['from_block']==seed['from_block'] and r['query']['to_block']==seed['to_block'])
    total=int(record['summary']['total']);qid=record['summary']['query_id']
    def call(query_id,budget):
        sent=subprocess.run(['cast','send','--no-proxy','--json','--rpc-url',dep['endpoint'],
            '--private-key','0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80',
            monitor,'check_budget(string,uint256)',query_id,str(budget)],cwd=ROOT,env=ENV,text=True,capture_output=True,timeout=25)
        assert sent.returncode==0,sent.stderr
        receipt=json.loads(sent.stdout)
        assert int(str(receipt['status']),0)==1,receipt
        deadline=time.monotonic()+15
        while time.monotonic()<deadline:
            r=subprocess.run(['cast','call','--no-proxy','--rpc-url',dep['endpoint'],monitor,
                'get_check_json(string,uint256)(string)',query_id,str(budget)],cwd=ROOT,env=ENV,text=True,capture_output=True,timeout=10)
            if r.returncode==0:
                decoded=json.loads(r.stdout.strip())
                value=json.loads(decoded) if isinstance(decoded,str) else decoded
                if value is not None:return value
            time.sleep(.2)
        raise AssertionError('consumer network decision never became visible')
    rows=[]
    for name,budget,expected in [('above_budget',total-1,True),('equal_budget',total,False),('below_budget',total+1,False)]:
        value=call(qid,budget)
        assert value['status']=='confirmed' and value['budget_exceeded']==expected and value['total']==str(total),value
        rows.append({'case':name,'passed':True,'result':value})
        print('PASS',name)
    rejected=call('0x'+'00'*32,1)
    assert rejected['status']=='rejected' and 'budget_exceeded' not in rejected,rejected
    rows.append({'case':'missing_query_rejected','passed':True,'result':rejected})
    output=ROOT/'output/results/consumer.json';output.parent.mkdir(parents=True,exist_ok=True)
    output.write_text(json.dumps({'kind':'real-cross-lyquid-read','monitor':monitor,'source':dep['lyquid_id'],'tests':rows},indent=2)+'\n')
    print('4/4 passed;',output)
if __name__=='__main__':
    try:main()
    except Exception as e:print('Consumer check failed:',e,file=sys.stderr);sys.exit(1)
